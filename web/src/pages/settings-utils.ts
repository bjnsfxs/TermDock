export function validatePortInput(rawPort: string): { ok: true; port: number } | { ok: false; message: string } {
  const parsedPort = Number(rawPort);
  if (!Number.isInteger(parsedPort) || parsedPort <= 0 || parsedPort > 65535) {
    return { ok: false, message: "port must be an integer in [1, 65535]." };
  }
  return { ok: true, port: parsedPort };
}
