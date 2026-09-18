#!/bin/sh
# Serve docs/ — the browser demo GitHub Pages publishes — from this machine, so
# a change to index.html can be looked at before it is committed and live at
# keyboardwarrior.app. Nothing here builds anything: the wasm, the JS and the
# songs under docs/ arrive from the private source repo's scripts/sync_web.sh,
# and this only puts a web server in front of what is already on disk.
#
#   sh scripts/serve.sh          http://127.0.0.1:8080
#   sh scripts/serve.sh 9000     a different port
#   sh scripts/serve.sh --lan    answer on the local network too, for a phone
#
# 8080 rather than 8000 on purpose. The source repo's build_web.sh serves its
# own freshly built copy on 8000, and that is a *different* build of this same
# page: the one that has not been synced here yet. Two servers fighting over
# one port is the good outcome — the bad one is reading the other build's page
# and believing it is this one.
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

# Run from anywhere else and the server would come up empty, or worse, serve
# whatever happened to be in the directory it started in.
[ -f docs/index.html ] || {
    echo "error: docs/index.html not found — is this the keyboardwarrior checkout?" >&2
    exit 1
}

# Best effort, and only used to print a URL. An empty answer is not a failure:
# the page is still served, the address just has to be found another way.
lan_ip() {
    ipconfig getifaddr en0 2>/dev/null && return 0
    ipconfig getifaddr en1 2>/dev/null && return 0
    hostname -I 2>/dev/null | awk '{ print $1 }'
}

ip=''
[ "$lan" -eq 1 ] && ip=$(lan_ip || true)

# -u so the address is on screen before serve_forever blocks, whether that is a
# terminal or something reading the script's output.
python3 -u - "$host" "$port" "$ip" <<'PY'
import errno
import http.server
import mimetypes
import socketserver
import sys

host, port, lan_ip = sys.argv[1], int(sys.argv[2]), sys.argv[3]

# WebAssembly.instantiateStreaming refuses anything that is not
# application/wasm, and whether an interpreter already knows that extension
# depends on its version and on the system mime table underneath it. Saying so
# here costs nothing and takes the question off the table.
mimetypes.add_type('application/wasm', '.wasm')


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory='docs', **kwargs)

    # The only reason to run this is that something changed, so a cached
    # response is the one answer that cannot be useful.
    def end_headers(self):
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()

    # One line per request is noise once the songs start loading.
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
        f"       'lsof -nP -iTCP:{port} -sTCP:LISTEN' names what holds it, or\n"
        f"       'sh scripts/serve.sh {port + 1}' goes around it."
    )

print(f'serving docs/ on http://127.0.0.1:{port}/')
if lan_ip:
    print(f'                http://{lan_ip}:{port}/  (this network)')
elif host == '0.0.0.0':
    print('                and on this machine\'s LAN address, which could not be read here')

if host == '0.0.0.0':
    # Worth saying before anyone concludes the button is broken: Web Share and
    # the clipboard are both restricted to secure contexts, and a plain http
    # LAN address is not one, while localhost is exempt. Layout, copy and tap
    # targets are all still worth checking on the real device.
    print('note: over a LAN address the page is not a secure context, so the')
    print('      Share button stays hidden — navigator.share and the clipboard')
    print('      are https-or-localhost only. The rest of the page is unaffected.')

print('ctrl-c to stop')

try:
    httpd.serve_forever()
except KeyboardInterrupt:
    print()
finally:
    httpd.server_close()
PY
