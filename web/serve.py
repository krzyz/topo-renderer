#!/usr/bin/env python3
from http.server import HTTPServer, SimpleHTTPRequestHandler, test
import sys

class RequestHandler(SimpleHTTPRequestHandler):
    def do_GET(self):
        print(self.path)
        if self.path.endswith(('.js', '.css', '.wasm', '.txt')):
            super().do_GET()
        else:
            self.send_response(200)
            self.send_header( 'Content-type', 'text/html' )
            self.end_headers()
            self.wfile.write( bytes(open('index.html').read(), encoding='utf-8') )

    def end_headers(self):
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        SimpleHTTPRequestHandler.end_headers(self)

if __name__ == '__main__':
    test(RequestHandler, HTTPServer, port=int(sys.argv[1]) if len(sys.argv) > 1 else 8080)
