use crate::{
    error::AppError,
    models::{now_rfc3339, Instance, InstanceRuntime, InstanceStatus},
};
use portable_pty::{
    native_pty_system, Child as PtyChild, CommandBuilder as PtyCommandBuilder, MasterPty, PtySize,
};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    process::Stdio,
    sync::{Arc, Mutex as StdMutex},
};
use sysinfo::{Pid, ProcessRefreshKind, System};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
    sync::{broadcast, Mutex},
    task::JoinHandle,
    time::{timeout, Duration},
};
use uuid::Uuid;

const OUTPUT_RING_CAPACITY_BYTES: usize = 1024 * 1024; // 1 MiB
const OUTPUT_BROADCAST_CAPACITY_FRAMES: usize = 256;
const OUTPUT_READ_CHUNK_BYTES: usize = 4096;
const GRACEFUL_STOP_TIMEOUT_SECS: u64 = 2;
const FORCE_STOP_TIMEOUT_SECS: u64 = 3;
const DEFAULT_PTY_ROWS: u16 = 24;
const DEFAULT_PTY_COLS: u16 = 80;
const EVENT_BROADCAST_CAPACITY_FRAMES: usize = 256;

pub struct TailOutput {
    pub data: Vec<u8>,
    pub truncated: bool,
}

pub struct TerminalAttach {
    pub runtime: InstanceRuntime,
    pub backend: &'static str,
    pub output_rx: broadcast::Receiver<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ProcessEvent {
    #[serde(rename = "instance_status")]
    InstanceStatus { id: Uuid, runtime: InstanceRuntime },
}

impl ProcessEvent {
    fn instance_status(id: Uuid, runtime: InstanceRuntime) -> Self {
        Self::InstanceStatus { id, runtime }
    }
}

/// ProcessManager handles per-instance process state and output ring buffers.
pub struct ProcessManager {
    entries: Mutex<HashMap<Uuid, Arc<InstanceEntry>>>,
    pipes_backend: PipesBackend,
    pty_backend: PtyBackend,
    output_sink: Arc<dyn OutputSink>,
    event_tx: broadcast::Sender<ProcessEvent>,
    metrics_system: Mutex<System>,
}

impl ProcessManager {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_BROADCAST_CAPACITY_FRAMES);
        Self {
            entries: Mutex::new(HashMap::new()),
            pipes_backend: PipesBackend,
            pty_backend: PtyBackend,
            output_sink: Arc::new(NoopOutputSink),
            event_tx,
            metrics_system: Mutex::new(System::new()),
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<ProcessEvent> {
        self.event_tx.subscribe()
    }

    /// Get latest runtime snapshot for an instance.
    pub async fn runtime(&self, id: Uuid) -> Option<InstanceRuntime> {
        let entry = self.get_entry(id).await?;

        let mut state = entry.state.lock().await;
        if refresh_entry_state(&mut state) {
            self.publish_runtime_if_changed(id, &mut state);
        }
        Some(state.runtime.clone())
    }

    /// Start an instance using selected backend.
    pub async fn start(&self, instance: &Instance) -> Result<InstanceRuntime, AppError> {
        let id = instance.id;
        let entry = self.get_or_create_entry(id).await;
        let _op = entry.op_lock.lock().await;

        {
            let mut state = entry.state.lock().await;
            if refresh_entry_state(&mut state) {
                self.publish_runtime_if_changed(id, &mut state);
            }
            if state.process.is_some() {
                return Err(AppError::conflict("instance already running"));
            }
            state.runtime.status = InstanceStatus::Starting;
            state.runtime.pid = None;
            state.runtime.started_at = None;
            state.runtime.exit_code = None;
            state.runtime.clients_attached = 0;
            clear_runtime_metrics(&mut state.runtime);
            self.publish_runtime_if_changed(id, &mut state);
        }

        let backend: &dyn InstanceBackend = if instance.use_pty {
            &self.pty_backend
        } else {
            &self.pipes_backend
        };

        let started = match backend.start(instance) {
            Ok(v) => v,
            Err(err) => {
                let mut state = entry.state.lock().await;
                state.runtime.status = InstanceStatus::Error;
                state.runtime.pid = None;
                state.runtime.started_at = None;
                state.runtime.exit_code = None;
                state.runtime.clients_attached = 0;
                clear_runtime_metrics(&mut state.runtime);
                self.publish_runtime_if_changed(id, &mut state);
                return Err(err);
            }
        };

        let pid = started.child.pid();
        let reader_tasks = self.spawn_output_readers(
            id,
            Arc::clone(&entry),
            started.stdout,
            started.stderr,
            started.blocking_reader,
        );

        let mut state = entry.state.lock().await;
        state.process = Some(ManagedProcess {
            backend: started.backend,
            child: started.child,
            input: started.input,
            pty_master: started.pty_master,
            reader_tasks,
        });
        state.runtime.status = InstanceStatus::Running;
        state.runtime.pid = pid;
        state.runtime.started_at = Some(now_rfc3339());
        state.runtime.exit_code = None;
        state.runtime.clients_attached = 0;
        clear_runtime_metrics(&mut state.runtime);
        self.publish_runtime_if_changed(id, &mut state);
        Ok(state.runtime.clone())
    }

    /// Stop an instance using graceful -> forceful termination.
    pub async fn stop(&self, id: Uuid) -> Result<InstanceRuntime, AppError> {
        let entry = self.get_or_create_entry(id).await;
        let _op = entry.op_lock.lock().await;

        let mut managed = {
            let mut state = entry.state.lock().await;
            if refresh_entry_state(&mut state) {
                self.publish_runtime_if_changed(id, &mut state);
            }

            let Some(process) = state.process.take() else {
                state.runtime.status = InstanceStatus::Stopped;
                state.runtime.pid = None;
                state.runtime.started_at = None;
                state.runtime.exit_code = None;
                state.runtime.clients_attached = 0;
                clear_runtime_metrics(&mut state.runtime);
                self.publish_runtime_if_changed(id, &mut state);
                return Ok(state.runtime.clone());
            };

            state.runtime.status = InstanceStatus::Stopping;
            self.publish_runtime_if_changed(id, &mut state);
            process
        };

        managed.close_input().await;
        let exit_code = stop_managed_child(managed.child, id).await;
        managed.pty_master.take();
        abort_reader_tasks(&mut managed.reader_tasks);

        let mut state = entry.state.lock().await;
        state.runtime.status = InstanceStatus::Stopped;
        state.runtime.pid = None;
        state.runtime.started_at = None;
        state.runtime.exit_code = exit_code;
        state.runtime.clients_attached = 0;
        clear_runtime_metrics(&mut state.runtime);
        self.publish_runtime_if_changed(id, &mut state);
        Ok(state.runtime.clone())
    }

    pub async fn restart(&self, instance: &Instance) -> Result<InstanceRuntime, AppError> {
        let _ = self.stop(instance.id).await?;
        self.start(instance).await
    }

    pub async fn tail_output(&self, id: Uuid, bytes: usize) -> Result<TailOutput, AppError> {
        let Some(entry) = self.get_entry(id).await else {
            return Ok(TailOutput {
                data: Vec::new(),
                truncated: true,
            });
        };

        Ok(entry.tail(bytes))
    }

    pub async fn attach_terminal(&self, id: Uuid) -> Result<TerminalAttach, AppError> {
        let Some(entry) = self.get_entry(id).await else {
            return Err(AppError::conflict("instance is not running"));
        };

        let mut state = entry.state.lock().await;
        if refresh_entry_state(&mut state) {
            self.publish_runtime_if_changed(id, &mut state);
        }
        let Some(backend) = state
            .process
            .as_ref()
            .map(|process| process.backend.as_str())
        else {
            return Err(AppError::conflict("instance is not running"));
        };

        state.runtime.clients_attached = state.runtime.clients_attached.saturating_add(1);
        self.publish_runtime_if_changed(id, &mut state);
        Ok(TerminalAttach {
            runtime: state.runtime.clone(),
            backend,
            output_rx: entry.subscribe_output(),
        })
    }

    pub async fn detach_terminal(&self, id: Uuid) {
        let Some(entry) = self.get_entry(id).await else {
            return;
        };

        let mut state = entry.state.lock().await;
        if refresh_entry_state(&mut state) {
            self.publish_runtime_if_changed(id, &mut state);
        }
        state.runtime.clients_attached = state.runtime.clients_attached.saturating_sub(1);
        self.publish_runtime_if_changed(id, &mut state);
    }

    pub async fn write_input(&self, id: Uuid, data: &[u8]) -> Result<(), AppError> {
        if data.is_empty() {
            return Ok(());
        }

        let Some(entry) = self.get_entry(id).await else {
            return Err(AppError::conflict("instance is not running"));
        };

        let input = {
            let mut state = entry.state.lock().await;
            if refresh_entry_state(&mut state) {
                self.publish_runtime_if_changed(id, &mut state);
            }
            let Some(process) = state.process.as_ref() else {
                return Err(AppError::conflict("instance is not running"));
            };
            process.input.clone()
        };

        let Some(input) = input else {
            return Err(AppError::conflict("instance stdin is unavailable"));
        };

        match input {
            InputWriter::Pipes(stdin) => {
                let mut guard = stdin.lock().await;
                let Some(stdin) = guard.as_mut() else {
                    return Err(AppError::conflict("instance stdin is closed"));
                };
                stdin.write_all(data).await?;
                stdin.flush().await?;
            }
            InputWriter::Pty(writer) => {
                let payload = data.to_vec();
                let write_res = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                    let mut guard = writer.lock().expect("pty writer lock poisoned");
                    let Some(writer) = guard.as_mut() else {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "pty writer is closed",
                        ));
                    };
                    writer.write_all(&payload)?;
                    writer.flush()?;
                    Ok(())
                })
                .await
                .map_err(|err| AppError::internal(format!("pty stdin write join failed: {err}")))?;

                write_res?;
            }
        }

        Ok(())
    }

    pub async fn resize_terminal(&self, id: Uuid, cols: u16, rows: u16) -> Result<(), AppError> {
        if cols == 0 || rows == 0 {
            return Err(AppError::bad_request("cols and rows must be positive"));
        }

        let Some(entry) = self.get_entry(id).await else {
            return Err(AppError::conflict("instance is not running"));
        };

        let pty_master = {
            let mut state = entry.state.lock().await;
            if refresh_entry_state(&mut state) {
                self.publish_runtime_if_changed(id, &mut state);
            }
            let Some(process) = state.process.as_ref() else {
                return Err(AppError::conflict("instance is not running"));
            };
            process.pty_master.clone()
        };

        let Some(master) = pty_master else {
            // Pipes backend: resize is a no-op.
            return Ok(());
        };

        tokio::task::spawn_blocking(move || {
            let guard = master.lock().expect("pty master lock poisoned");
            guard
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|err| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("pty resize failed: {err}"),
                    )
                })
        })
        .await
        .map_err(|err| AppError::internal(format!("pty resize join failed: {err}")))??;

        Ok(())
    }

    pub async fn sample_metrics_once(&self) {
        let entries: Vec<(Uuid, Arc<InstanceEntry>)> = {
            let guard = self.entries.lock().await;
            guard
                .iter()
                .map(|(id, entry)| (*id, Arc::clone(entry)))
                .collect()
        };

        if entries.is_empty() {
            return;
        }

        let mut running_entries: Vec<(Uuid, Arc<InstanceEntry>, u32)> = Vec::new();
        for (id, entry) in &entries {
            let mut state = entry.state.lock().await;
            let mut changed = refresh_entry_state(&mut state);

            if matches!(state.runtime.status, InstanceStatus::Running) {
                if let Some(pid) = state.runtime.pid {
                    running_entries.push((*id, Arc::clone(entry), pid));
                } else if state.runtime.cpu_percent.is_some() || state.runtime.mem_bytes.is_some() {
                    clear_runtime_metrics(&mut state.runtime);
                    changed = true;
                }
            } else if state.runtime.cpu_percent.is_some() || state.runtime.mem_bytes.is_some() {
                clear_runtime_metrics(&mut state.runtime);
                changed = true;
            }

            if changed {
                self.publish_runtime_if_changed(*id, &mut state);
            }
        }

        if running_entries.is_empty() {
            return;
        }

        let pids: Vec<Pid> = running_entries
            .iter()
            .map(|(_, _, pid)| Pid::from_u32(*pid))
            .collect();
        let mut metrics_by_pid: HashMap<u32, (Option<f32>, Option<u64>)> = HashMap::new();
        {
            let mut system = self.metrics_system.lock().await;
            system
                .refresh_pids_specifics(&pids, ProcessRefreshKind::new().with_cpu().with_memory());
            for pid in &pids {
                let metrics = system
                    .process(*pid)
                    .map(|process| (Some(process.cpu_usage()), Some(process.memory())))
                    .unwrap_or((None, None));
                metrics_by_pid.insert(pid.as_u32(), metrics);
            }
        }

        for (id, entry, pid_u32) in running_entries {
            let (cpu_percent, mem_bytes) = metrics_by_pid
                .get(&pid_u32)
                .copied()
                .unwrap_or((None, None));

            let mut state = entry.state.lock().await;
            let mut changed = refresh_entry_state(&mut state);

            if matches!(state.runtime.status, InstanceStatus::Running)
                && state.runtime.pid == Some(pid_u32)
            {
                if state.runtime.cpu_percent != cpu_percent {
                    state.runtime.cpu_percent = cpu_percent;
                    changed = true;
                }
                if state.runtime.mem_bytes != mem_bytes {
                    state.runtime.mem_bytes = mem_bytes;
                    changed = true;
                }
            } else if state.runtime.cpu_percent.is_some() || state.runtime.mem_bytes.is_some() {
                clear_runtime_metrics(&mut state.runtime);
                changed = true;
            }

            if changed {
                self.publish_runtime_if_changed(id, &mut state);
            }
        }
    }

    fn publish_runtime_if_changed(&self, id: Uuid, state: &mut EntryState) {
        if state.last_published_runtime.as_ref() == Some(&state.runtime) {
            return;
        }

        let snapshot = state.runtime.clone();
        state.last_published_runtime = Some(snapshot.clone());
        let _ = self
            .event_tx
            .send(ProcessEvent::instance_status(id, snapshot));
    }

    async fn get_entry(&self, id: Uuid) -> Option<Arc<InstanceEntry>> {
        self.entries.lock().await.get(&id).cloned()
    }

    async fn get_or_create_entry(&self, id: Uuid) -> Arc<InstanceEntry> {
        let mut entries = self.entries.lock().await;
        entries
            .entry(id)
            .or_insert_with(|| Arc::new(InstanceEntry::new()))
            .clone()
    }

    fn spawn_output_readers(
        &self,
        id: Uuid,
        entry: Arc<InstanceEntry>,
        stdout: Option<ChildStdout>,
        stderr: Option<ChildStderr>,
        blocking_reader: Option<Box<dyn Read + Send>>,
    ) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::with_capacity(3);
        if let Some(out) = stdout {
            handles.push(spawn_async_reader_task(
                out,
                id,
                Arc::clone(&entry),
                Arc::clone(&self.output_sink),
            ));
        }
        if let Some(err) = stderr {
            handles.push(spawn_async_reader_task(
                err,
                id,
                Arc::clone(&entry),
                Arc::clone(&self.output_sink),
            ));
        }
        if let Some(reader) = blocking_reader {
            handles.push(spawn_blocking_reader_task(
                reader,
                id,
                entry,
                Arc::clone(&self.output_sink),
            ));
        }
        handles
    }
}

fn spawn_async_reader_task<R>(
    mut reader: R,
    id: Uuid,
    entry: Arc<InstanceEntry>,
    output_sink: Arc<dyn OutputSink>,
) -> JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buf = vec![0u8; OUTPUT_READ_CHUNK_BYTES];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    entry.append_output(chunk);
                    output_sink.on_output(id, chunk);
                }
                Err(err) => {
                    tracing::warn!(instance_id = %id, error = %err, "output reader failed");
                    break;
                }
            }
        }
    })
}

fn spawn_blocking_reader_task(
    mut reader: Box<dyn Read + Send>,
    id: Uuid,
    entry: Arc<InstanceEntry>,
    output_sink: Arc<dyn OutputSink>,
) -> JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        let mut buf = vec![0u8; OUTPUT_READ_CHUNK_BYTES];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    entry.append_output(chunk);
                    output_sink.on_output(id, chunk);
                }
                Err(err) => {
                    tracing::warn!(instance_id = %id, error = %err, "pty output reader failed");
                    break;
                }
            }
        }
    })
}

fn abort_reader_tasks(tasks: &mut Vec<JoinHandle<()>>) {
    for handle in tasks.drain(..) {
        handle.abort();
    }
}

async fn stop_managed_child(child: ProcessChild, id: Uuid) -> Option<i32> {
    match child {
        ProcessChild::Pipes(mut child) => stop_pipes_child(&mut child, id).await,
        ProcessChild::Pty(child) => stop_pty_child(child, id).await,
    }
}

async fn stop_pipes_child(child: &mut Child, id: Uuid) -> Option<i32> {
    match timeout(
        Duration::from_secs(GRACEFUL_STOP_TIMEOUT_SECS),
        child.wait(),
    )
    .await
    {
        Ok(Ok(status)) => status.code(),
        Ok(Err(err)) => {
            tracing::warn!(instance_id = %id, error = %err, "failed to wait pipes process");
            None
        }
        Err(_) => {
            let _ = child.start_kill();
            match timeout(Duration::from_secs(FORCE_STOP_TIMEOUT_SECS), child.wait()).await {
                Ok(Ok(status)) => status.code(),
                Ok(Err(err)) => {
                    tracing::warn!(
                        instance_id = %id,
                        error = %err,
                        "failed to wait pipes process after kill"
                    );
                    None
                }
                Err(_) => {
                    tracing::warn!(instance_id = %id, "timed out waiting pipes process after kill");
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    None
                }
            }
        }
    }
}

async fn stop_pty_child(mut child: Box<dyn PtyChild + Send + Sync>, id: Uuid) -> Option<i32> {
    let mut killer = child.clone_killer();
    let mut wait_task = tokio::task::spawn_blocking(move || child.wait());

    match timeout(
        Duration::from_secs(GRACEFUL_STOP_TIMEOUT_SECS),
        &mut wait_task,
    )
    .await
    {
        Ok(Ok(Ok(status))) => Some(pty_exit_code_to_i32(status.exit_code())),
        Ok(Ok(Err(err))) => {
            tracing::warn!(instance_id = %id, error = %err, "failed to wait pty process");
            None
        }
        Ok(Err(err)) => {
            tracing::warn!(instance_id = %id, error = %err, "pty wait join failed");
            None
        }
        Err(_) => {
            let _ = killer.kill();
            match timeout(Duration::from_secs(FORCE_STOP_TIMEOUT_SECS), &mut wait_task).await {
                Ok(Ok(Ok(status))) => Some(pty_exit_code_to_i32(status.exit_code())),
                Ok(Ok(Err(err))) => {
                    tracing::warn!(
                        instance_id = %id,
                        error = %err,
                        "failed to wait pty process after kill"
                    );
                    None
                }
                Ok(Err(err)) => {
                    tracing::warn!(instance_id = %id, error = %err, "pty wait join failed after kill");
                    None
                }
                Err(_) => {
                    tracing::warn!(instance_id = %id, "timed out waiting pty process after kill");
                    let _ = killer.kill();
                    None
                }
            }
        }
    }
}

fn refresh_process_state(
    runtime: &mut InstanceRuntime,
    process: &mut Option<ManagedProcess>,
) -> bool {
    let before = runtime.clone();
    let mut finalize: Option<Option<i32>> = None;
    let mut mark_error = false;

    if let Some(managed) = process.as_mut() {
        match managed.child.try_wait() {
            Ok(TryWaitResult::Running) => {}
            Ok(TryWaitResult::Exited(exit_code)) => finalize = Some(exit_code),
            Err(err) => {
                tracing::warn!(error = %err, "failed to refresh child status");
                mark_error = true;
            }
        }
    }

    if mark_error {
        if let Some(mut managed) = process.take() {
            abort_reader_tasks(&mut managed.reader_tasks);
        }
        runtime.status = InstanceStatus::Error;
        runtime.pid = None;
        runtime.exit_code = None;
        runtime.clients_attached = 0;
        clear_runtime_metrics(runtime);
        return *runtime != before;
    }

    let Some(exit_code) = finalize else {
        return false;
    };

    if let Some(mut managed) = process.take() {
        abort_reader_tasks(&mut managed.reader_tasks);
    }

    runtime.status = match exit_code {
        Some(0) | None => InstanceStatus::Exited,
        Some(_) => InstanceStatus::Error,
    };
    runtime.pid = None;
    runtime.exit_code = exit_code;
    runtime.clients_attached = 0;
    clear_runtime_metrics(runtime);
    *runtime != before
}

fn refresh_entry_state(state: &mut EntryState) -> bool {
    let EntryState {
        runtime, process, ..
    } = state;
    refresh_process_state(runtime, process)
}

fn clear_runtime_metrics(runtime: &mut InstanceRuntime) {
    runtime.cpu_percent = None;
    runtime.mem_bytes = None;
}

struct InstanceEntry {
    op_lock: Mutex<()>,
    state: Mutex<EntryState>,
    output: StdMutex<ByteRing>,
    output_tx: broadcast::Sender<Vec<u8>>,
}

impl InstanceEntry {
    fn new() -> Self {
        let (output_tx, _) = broadcast::channel(OUTPUT_BROADCAST_CAPACITY_FRAMES);
        Self {
            op_lock: Mutex::new(()),
            state: Mutex::new(EntryState {
                runtime: InstanceRuntime::default(),
                process: None,
                last_published_runtime: None,
            }),
            output: StdMutex::new(ByteRing::new(OUTPUT_RING_CAPACITY_BYTES)),
            output_tx,
        }
    }

    fn append_output(&self, chunk: &[u8]) {
        {
            let mut ring = self.output.lock().expect("output ring lock poisoned");
            ring.push(chunk);
        }
        let _ = self.output_tx.send(chunk.to_vec());
    }

    fn tail(&self, bytes: usize) -> TailOutput {
        let ring = self.output.lock().expect("output ring lock poisoned");
        ring.tail(bytes)
    }

    fn subscribe_output(&self) -> broadcast::Receiver<Vec<u8>> {
        self.output_tx.subscribe()
    }
}

struct EntryState {
    runtime: InstanceRuntime,
    process: Option<ManagedProcess>,
    last_published_runtime: Option<InstanceRuntime>,
}

struct ManagedProcess {
    backend: ProcessBackend,
    child: ProcessChild,
    input: Option<InputWriter>,
    pty_master: Option<Arc<StdMutex<Box<dyn MasterPty + Send>>>>,
    reader_tasks: Vec<JoinHandle<()>>,
}

impl ManagedProcess {
    async fn close_input(&mut self) {
        let Some(input) = self.input.take() else {
            return;
        };

        match input {
            InputWriter::Pipes(stdin) => {
                let mut guard = stdin.lock().await;
                guard.take();
            }
            InputWriter::Pty(writer) => {
                let mut guard = writer.lock().expect("pty writer lock poisoned");
                guard.take();
            }
        }
    }
}

#[derive(Clone)]
enum InputWriter {
    Pipes(Arc<Mutex<Option<ChildStdin>>>),
    Pty(Arc<StdMutex<Option<Box<dyn Write + Send>>>>),
}

enum ProcessChild {
    Pipes(Child),
    Pty(Box<dyn PtyChild + Send + Sync>),
}

enum TryWaitResult {
    Running,
    Exited(Option<i32>),
}

impl ProcessChild {
    fn pid(&self) -> Option<u32> {
        match self {
            ProcessChild::Pipes(child) => child.id(),
            ProcessChild::Pty(child) => child.process_id(),
        }
    }

    fn try_wait(&mut self) -> std::io::Result<TryWaitResult> {
        match self {
            ProcessChild::Pipes(child) => match child.try_wait()? {
                Some(status) => Ok(TryWaitResult::Exited(status.code())),
                None => Ok(TryWaitResult::Running),
            },
            ProcessChild::Pty(child) => match child.try_wait()? {
                Some(status) => Ok(TryWaitResult::Exited(Some(pty_exit_code_to_i32(
                    status.exit_code(),
                )))),
                None => Ok(TryWaitResult::Running),
            },
        }
    }
}

#[derive(Clone, Copy)]
enum ProcessBackend {
    Pipes,
    Pty,
}

impl ProcessBackend {
    fn as_str(self) -> &'static str {
        match self {
            ProcessBackend::Pipes => "pipes",
            ProcessBackend::Pty => "pty",
        }
    }
}

fn pty_exit_code_to_i32(code: u32) -> i32 {
    i32::try_from(code).unwrap_or(i32::MAX)
}

struct ByteRing {
    capacity: usize,
    buf: VecDeque<u8>,
}

impl ByteRing {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            buf: VecDeque::with_capacity(capacity),
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        if chunk.is_empty() || self.capacity == 0 {
            return;
        }

        if chunk.len() >= self.capacity {
            self.buf.clear();
            self.buf
                .extend(chunk[chunk.len() - self.capacity..].iter().copied());
            return;
        }

        while self.buf.len() + chunk.len() > self.capacity {
            let _ = self.buf.pop_front();
        }
        self.buf.extend(chunk.iter().copied());
    }

    fn tail(&self, requested: usize) -> TailOutput {
        if requested == 0 {
            return TailOutput {
                data: Vec::new(),
                truncated: false,
            };
        }

        let available = self.buf.len();
        let take = available.min(requested);
        let start = available.saturating_sub(take);
        let mut data = Vec::with_capacity(take);
        for b in self.buf.iter().skip(start) {
            data.push(*b);
        }

        TailOutput {
            data,
            truncated: available < requested,
        }
    }
}

trait OutputSink: Send + Sync {
    fn on_output(&self, instance_id: Uuid, chunk: &[u8]);
}

struct NoopOutputSink;

impl OutputSink for NoopOutputSink {
    fn on_output(&self, _instance_id: Uuid, _chunk: &[u8]) {}
}

struct StartedProcess {
    backend: ProcessBackend,
    child: ProcessChild,
    input: Option<InputWriter>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    blocking_reader: Option<Box<dyn Read + Send>>,
    pty_master: Option<Arc<StdMutex<Box<dyn MasterPty + Send>>>>,
}

trait InstanceBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn start(&self, instance: &Instance) -> Result<StartedProcess, AppError>;
}

struct PipesBackend;

impl InstanceBackend for PipesBackend {
    fn name(&self) -> &'static str {
        "pipes"
    }

    fn start(&self, instance: &Instance) -> Result<StartedProcess, AppError> {
        let mut cmd = Command::new(&instance.command);
        cmd.args(&instance.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(cwd) = instance.cwd.as_ref().filter(|v| !v.trim().is_empty()) {
            cmd.current_dir(cwd);
        }

        for (k, v) in &instance.env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|err| {
            AppError::internal(format!(
                "failed to start process with {} backend: {err}",
                self.name()
            ))
        })?;

        let input = child
            .stdin
            .take()
            .map(|stdin| InputWriter::Pipes(Arc::new(Mutex::new(Some(stdin)))));
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        Ok(StartedProcess {
            backend: ProcessBackend::Pipes,
            child: ProcessChild::Pipes(child),
            input,
            stdout,
            stderr,
            blocking_reader: None,
            pty_master: None,
        })
    }
}

struct PtyBackend;

impl InstanceBackend for PtyBackend {
    fn name(&self) -> &'static str {
        "pty"
    }

    fn start(&self, instance: &Instance) -> Result<StartedProcess, AppError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: DEFAULT_PTY_ROWS,
                cols: DEFAULT_PTY_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| {
                AppError::internal(format!(
                    "failed to open pty with {} backend: {err}",
                    self.name()
                ))
            })?;

        let mut cmd = PtyCommandBuilder::new(&instance.command);
        cmd.args(&instance.args);
        if let Some(cwd) = instance.cwd.as_ref().filter(|v| !v.trim().is_empty()) {
            cmd.cwd(cwd);
        }
        for (k, v) in &instance.env {
            cmd.env(k, v);
        }

        let child = pair.slave.spawn_command(cmd).map_err(|err| {
            AppError::internal(format!(
                "failed to spawn process with {} backend: {err}",
                self.name()
            ))
        })?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().map_err(|err| {
            AppError::internal(format!(
                "failed to clone pty reader with {} backend: {err}",
                self.name()
            ))
        })?;
        let writer = pair.master.take_writer().map_err(|err| {
            AppError::internal(format!(
                "failed to take pty writer with {} backend: {err}",
                self.name()
            ))
        })?;
        let master = Arc::new(StdMutex::new(pair.master));

        Ok(StartedProcess {
            backend: ProcessBackend::Pty,
            child: ProcessChild::Pty(child),
            input: Some(InputWriter::Pty(Arc::new(StdMutex::new(Some(writer))))),
            stdout: None,
            stderr: None,
            blocking_reader: Some(reader),
            pty_master: Some(master),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConfigMode, RestartPolicy};
    use std::collections::BTreeMap;
    use tokio::time::timeout;

    fn test_instance(use_pty: bool) -> Instance {
        let (command, args) = if cfg!(windows) {
            (
                "cmd".to_string(),
                vec![
                    "/C".to_string(),
                    "echo hello && ping -n 6 127.0.0.1 > nul".to_string(),
                ],
            )
        } else {
            (
                "sh".to_string(),
                vec!["-c".to_string(), "printf hello; sleep 5".to_string()],
            )
        };

        Instance {
            id: Uuid::new_v4(),
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
            name: "test".to_string(),
            enabled: true,
            command,
            args,
            cwd: None,
            env: BTreeMap::new(),
            use_pty,
            config_mode: ConfigMode::None,
            config_path: None,
            config_filename: None,
            config_content: None,
            restart_policy: RestartPolicy::Never,
            auto_start: false,
            runtime: None,
        }
    }

    async fn recv_runtime_event(
        rx: &mut broadcast::Receiver<ProcessEvent>,
        instance_id: Uuid,
    ) -> InstanceRuntime {
        timeout(Duration::from_secs(5), async {
            loop {
                match rx.recv().await {
                    Ok(ProcessEvent::InstanceStatus { id, runtime }) if id == instance_id => {
                        return runtime;
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(err) => panic!("unexpected process event receiver error: {err}"),
                }
            }
        })
        .await
        .expect("timed out waiting for process event")
    }

    async fn recv_status(
        rx: &mut broadcast::Receiver<ProcessEvent>,
        instance_id: Uuid,
        status: InstanceStatus,
    ) -> InstanceRuntime {
        loop {
            let runtime = recv_runtime_event(rx, instance_id).await;
            if runtime.status == status {
                return runtime;
            }
        }
    }

    #[test]
    fn byte_ring_tail_semantics() {
        let mut ring = ByteRing::new(8);
        ring.push(b"abcdef");
        let tail = ring.tail(4);
        assert_eq!(tail.data, b"cdef");
        assert!(!tail.truncated);

        ring.push(b"ghijkl");
        let tail = ring.tail(8);
        assert_eq!(tail.data, b"efghijkl");
        assert!(!tail.truncated);

        let longer = ring.tail(32);
        assert_eq!(longer.data, b"efghijkl");
        assert!(longer.truncated);
    }

    #[tokio::test]
    async fn start_stop_pipes_and_collect_output() {
        let manager = ProcessManager::new();
        let instance = test_instance(false);

        let running = manager
            .start(&instance)
            .await
            .expect("start should succeed");
        assert!(matches!(running.status, InstanceStatus::Running));

        tokio::time::sleep(Duration::from_millis(200)).await;
        let out = manager
            .tail_output(instance.id, 1024)
            .await
            .expect("tail should succeed");
        assert!(!out.data.is_empty(), "expected captured output bytes");

        let stopped = manager
            .stop(instance.id)
            .await
            .expect("stop should succeed");
        assert!(matches!(stopped.status, InstanceStatus::Stopped));
    }

    #[tokio::test]
    async fn attach_and_detach_updates_client_count() {
        let manager = ProcessManager::new();
        let instance = test_instance(false);
        manager
            .start(&instance)
            .await
            .expect("start should succeed");

        let attach = manager
            .attach_terminal(instance.id)
            .await
            .expect("attach should succeed");
        assert_eq!(attach.backend, "pipes");
        assert_eq!(attach.runtime.clients_attached, 1);

        manager.detach_terminal(instance.id).await;
        let runtime = manager
            .runtime(instance.id)
            .await
            .expect("runtime should exist");
        assert_eq!(runtime.clients_attached, 0);

        let _ = manager
            .stop(instance.id)
            .await
            .expect("stop should succeed");
    }

    #[tokio::test]
    async fn start_and_stop_publish_status_events() {
        let manager = ProcessManager::new();
        let mut events = manager.subscribe_events();
        let instance = test_instance(false);

        manager
            .start(&instance)
            .await
            .expect("start should succeed");
        let running = recv_status(&mut events, instance.id, InstanceStatus::Running).await;
        assert!(running.pid.is_some());
        assert!(running.cpu_percent.is_none());
        assert!(running.mem_bytes.is_none());

        manager
            .stop(instance.id)
            .await
            .expect("stop should succeed");
        let stopped = recv_status(&mut events, instance.id, InstanceStatus::Stopped).await;
        assert!(stopped.pid.is_none());
        assert!(stopped.cpu_percent.is_none());
        assert!(stopped.mem_bytes.is_none());
    }

    #[tokio::test]
    async fn attach_and_detach_publish_client_count_events() {
        let manager = ProcessManager::new();
        let mut events = manager.subscribe_events();
        let instance = test_instance(false);

        manager
            .start(&instance)
            .await
            .expect("start should succeed");
        let _ = recv_status(&mut events, instance.id, InstanceStatus::Running).await;

        manager
            .attach_terminal(instance.id)
            .await
            .expect("attach should succeed");
        let mut attached = recv_runtime_event(&mut events, instance.id).await;
        while attached.clients_attached != 1 {
            attached = recv_runtime_event(&mut events, instance.id).await;
        }
        assert_eq!(attached.clients_attached, 1);

        manager.detach_terminal(instance.id).await;
        let mut detached = recv_runtime_event(&mut events, instance.id).await;
        while detached.clients_attached != 0 {
            detached = recv_runtime_event(&mut events, instance.id).await;
        }
        assert_eq!(detached.clients_attached, 0);

        let _ = manager
            .stop(instance.id)
            .await
            .expect("stop should succeed");
    }

    #[tokio::test]
    async fn runtime_refresh_does_not_publish_duplicate_event() {
        let manager = ProcessManager::new();
        let mut events = manager.subscribe_events();
        let instance = test_instance(false);

        manager
            .start(&instance)
            .await
            .expect("start should succeed");
        let _ = recv_status(&mut events, instance.id, InstanceStatus::Running).await;

        let _ = manager
            .runtime(instance.id)
            .await
            .expect("runtime should exist");

        let duplicate = timeout(Duration::from_millis(200), events.recv()).await;
        assert!(
            duplicate.is_err(),
            "expected no duplicate event for unchanged runtime"
        );

        let _ = manager
            .stop(instance.id)
            .await
            .expect("stop should succeed");
    }
}
