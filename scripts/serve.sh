#!/bin/sh
# Preview docs/ locally: sh scripts/serve.sh [port] [--lan]
set -eu
cd "$(dirname "$0")/.."

host=127.0.0.1
port=8080
lan=0

for arg in "$@"; do
    case "$arg" in
        --lan) lan=1; host=0.0.0.0 ;;
        [0-9]*) port=$arg ;;
        *)
            echo "usage: sh scripts/serve.sh [port] [--lan]" >&2
            exit 2
            ;;
    esac
done

[ -f docs/index.html ] || {
    echo "error: docs/index.html not found — is this the keyboardwarrior checkout?" >&2
    exit 1
}

lan_ip() {
    ipconfig getifaddr en0 2>/dev/null && return 0
    ipconfig getifaddr en1 2>/dev/null && return 0
    hostname -I 2>/dev/null | awk '{ print $1 }'
}

ip=''
[ "$lan" -eq 1 ] && ip=$(lan_ip || true)

python3 -u - "$host" "$port" "$ip" <<'PY'
import errno
import http.server
import mimetypes
import socketserver
import sys

host, port, lan_ip = sys.argv[1], int(sys.argv[2]), sys.argv[3]
mimetypes.add_type('application/wasm', '.wasm')


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory='docs', **kwargs)

    def end_headers(self):
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()

    def log_message(self, *args):
        pass


class Server(socketserver.TCPServer):
    allow_reuse_address = True


try:
    httpd = Server((host, port), Handler)
except OSError as err:
    if err.errno != errno.EADDRINUSE:
        raise
    sys.exit(
        f"error: port {port} is already in use.\n"
        f"Try: sh scripts/serve.sh {port + 1}"
    )

print(f'serving docs/ on http://127.0.0.1:{port}/')
if lan_ip:
    print(f'                http://{lan_ip}:{port}/  (this network)')
elif host == '0.0.0.0':
    print("LAN address unavailable; use this machine's local IP.")

if host == '0.0.0.0':
    print('Share and clipboard access require HTTPS or localhost.')

print('ctrl-c to stop')

try:
    httpd.serve_forever()
except KeyboardInterrupt:
    print()
finally:
    httpd.server_close()
PY
