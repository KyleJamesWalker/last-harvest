#!/usr/bin/env python3
"""Serve a web build of the editor, and open the native editor's remote door for it.

Usage: tools/editor_relay.py DIR [--port 8094] [--door 8792]

An agent's request to the door waits until the page, opened at http://127.0.0.1:PORT/?remote, fetches it from
/remote/next and posts its answer to /remote/reply/<id>.
"""

import argparse
import itertools
import queue
import threading
import time
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer, BaseHTTPRequestHandler

MAX_BODY = 4 << 20
ANSWER_WITHIN = 10.0
POLL_FOR = 20.0

calls: "queue.Queue[tuple[int, float, str, str, bytes]]" = queue.Queue()
pending: dict[int, tuple[threading.Event, list]] = {}
ids = itertools.count(1)
last_poll = 0.0


def host_ok(handler: BaseHTTPRequestHandler, port: int) -> bool:
    host = handler.headers.get("Host")
    return host is None or host in (f"127.0.0.1:{port}", f"localhost:{port}")


def body_of(handler: BaseHTTPRequestHandler) -> bytes:
    n = min(int(handler.headers.get("Content-Length") or 0), MAX_BODY)
    return handler.rfile.read(n) if n else b""


def respond(handler: BaseHTTPRequestHandler, status: int, kind: str, body: bytes, headers: dict[str, str] | None = None):
    handler.send_response(status)
    handler.send_header("Content-Type", kind)
    handler.send_header("Content-Length", str(len(body)))
    handler.send_header("Cache-Control", "no-store")
    for k, v in (headers or {}).items():
        handler.send_header(k, v)
    handler.end_headers()
    handler.wfile.write(body)


def say(handler: BaseHTTPRequestHandler, status: int, text: str):
    respond(handler, status, "text/plain; charset=utf-8", text.encode())


class Door(BaseHTTPRequestHandler):
    """The agent's side: the same calls as the native editor's door."""

    port = 0
    page = ""

    def log_message(self, *_):
        pass

    def any(self):
        # Browsers send Origin on writes, and a rebound DNS name shows in Host.
        if not host_ok(self, self.port) or self.headers.get("Origin"):
            return say(self, 403, "only local programs may drive the editor")
        body = body_of(self)
        if time.monotonic() - last_poll > POLL_FOR + 5:
            return say(self, 503, f"no editor page is listening: open {self.page}")
        n = next(ids)
        done, slot = threading.Event(), []
        pending[n] = (done, slot)
        calls.put((n, time.monotonic() + ANSWER_WITHIN, self.command, self.path, body))
        answered = done.wait(ANSWER_WITHIN)
        pending.pop(n, None)
        if not answered:
            return say(self, 503, "the editor didn't answer")
        respond(self, *slot[0])

    do_GET = do_PUT = do_POST = do_DELETE = any


class Page(SimpleHTTPRequestHandler):
    """The page's side: its files, and the requests waiting for it."""

    port = 0

    def log_message(self, *_):
        pass

    def same_origin(self) -> bool:
        # Every current browser marks its requests; another site's page can't pass for this one.
        return host_ok(self, self.port) and self.headers.get("Sec-Fetch-Site") == "same-origin"

    def do_GET(self):
        if not self.path.startswith("/remote/"):
            return super().do_GET()
        if self.path != "/remote/next" or not self.same_origin():
            return say(self, 403, "not this page")
        global last_poll
        last_poll = time.monotonic()
        until = last_poll + POLL_FOR
        while (left := until - time.monotonic()) > 0:
            try:
                n, deadline, method, path, body = calls.get(timeout=left)
            except queue.Empty:
                break
            if time.monotonic() < deadline and n in pending:
                return respond(self, 200, "application/octet-stream", body, {"X-Call": str(n), "X-Method": method, "X-Path": path})
        last_poll = time.monotonic()
        respond(self, 204, "text/plain", b"")

    def do_POST(self):
        n = self.path.removeprefix("/remote/reply/")
        if n == self.path or not n.isdigit() or not self.same_origin():
            return say(self, 403, "not this page")
        body = body_of(self)
        if waiting := pending.get(int(n)):
            done, slot = waiting
            status = int(self.headers.get("X-Status") or 500)
            slot.append((status, self.headers.get("Content-Type") or "application/octet-stream", body))
            done.set()
        say(self, 200, "ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("dir")
    ap.add_argument("--port", type=int, default=8094)
    ap.add_argument("--door", type=int, default=8792)
    a = ap.parse_args()
    Page.port, Door.port = a.port, a.door
    Door.page = f"http://127.0.0.1:{a.port}/?remote"
    page = ThreadingHTTPServer(("127.0.0.1", a.port), partial(Page, directory=a.dir))
    door = ThreadingHTTPServer(("127.0.0.1", a.door), Door)
    page.daemon_threads = door.daemon_threads = True
    threading.Thread(target=door.serve_forever, daemon=True).start()
    print(f"page: {Door.page}\nremote: http://127.0.0.1:{a.door}", flush=True)
    try:
        page.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
