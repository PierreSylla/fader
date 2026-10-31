#!/usr/bin/env python3
"""Spike jetable P0-5 : interroge Hyprland par ses sockets, sans passer par hyprctl.

Lecture seule : aucun dispatch, aucune écriture de config.
Usage : python3 spikes/hypr-ipc/probe.py [secondes d'écoute des événements]
"""
import json
import os
import socket
import sys
import time


def base():
    runtime = os.environ["XDG_RUNTIME_DIR"]
    return f"{runtime}/hypr/{os.environ['HYPRLAND_INSTANCE_SIGNATURE']}"


def request(command):
    with socket.socket(socket.AF_UNIX) as s:
        s.connect(f"{base()}/.socket.sock")
        t = time.perf_counter()
        s.sendall(command.encode())
        chunks = []
        while chunk := s.recv(65536):
            chunks.append(chunk)
    return b"".join(chunks).decode(), (time.perf_counter() - t) * 1000


def main():
    listen = float(sys.argv[1]) if len(sys.argv) > 1 else 5

    status, ms = request("j/status")
    print(f"j/status ({ms:.1f} ms) : {json.loads(status)}")

    monitors, ms = request("j/monitors all")
    print(f"j/monitors all ({ms:.1f} ms) :")
    for m in json.loads(monitors):
        print(f"  {m['name']} id={m['id']} {m['make']} | {m['model']} | {m['serial']} | {m['x']},{m['y']} {m['width']}x{m['height']} x{m['scale']} disabled={m['disabled']} dpms={m['dpmsStatus']} focused={m['focused']}")

    for command in ("j/commandeinconnue", "commandeinconnue", "status"):
        print(f"{command!r} -> {request(command)[0][:90]!r}")

    with socket.socket(socket.AF_UNIX) as s:
        s.connect(f"{base()}/.socket2.sock")
        s.settimeout(listen)
        print(f".socket2.sock : écoute {listen:.0f} s")
        names = {}
        data = b""
        end = time.time() + listen
        try:
            while time.time() < end:
                s.settimeout(max(end - time.time(), 0.01))
                data += s.recv(65536)
        except TimeoutError:
            pass
        for line in data.decode(errors="replace").splitlines():
            name, _, payload = line.partition(">>")
            names.setdefault(name, payload[:70])
        print(f"  événements distincts : {names or 'aucun'}")


if __name__ == "__main__":
    main()
