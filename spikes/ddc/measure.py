#!/usr/bin/env python3
"""Spike jetable P0-4 : latences et comportements de ddcutil sur les écrans réels.

N'écrit que VCP 0x10, à ±1 de la valeur initiale, et restaure en sortie.
Usage : python3 spikes/ddc/measure.py
"""
import re
import statistics as st
import subprocess
import threading
import time


def ddc(*args, timeout=30):
    t = time.perf_counter()
    try:
        p = subprocess.run(["ddcutil", *args], capture_output=True, text=True, timeout=timeout)
        return p.returncode, (p.stdout + p.stderr).strip(), (time.perf_counter() - t) * 1000
    except subprocess.TimeoutExpired:
        return -1, "TIMEOUT", timeout * 1000


def detect():
    _, out, ms = ddc("detect")
    found = re.findall(r"/dev/i2c-(\d+)\s+DRM_connector:\s+(\S+).*?Serial number:\s+(\S+)", out, re.S)
    return {int(b): (c, s) for b, c, s in found}, ms


def get(bus):
    code, out, ms = ddc("--bus", str(bus), "--brief", "getvcp", "10")
    m = re.search(r"VCP 10 C (\d+) (\d+)", out)
    return (int(m[1]), int(m[2]), ms) if m else (None, None, ms)


def set_(bus, value, *flags):
    return ddc("--bus", str(bus), "setvcp", "10", str(value), *flags)


def summary(values):
    values = sorted(values)
    return f"min {values[0]:.0f}, med {st.median(values):.0f}, max {values[-1]:.0f} ms (n={len(values)})"


def write_tests(bus, initial, maximum):
    other = initial - 1 if initial > 0 else initial + 1

    print(f"\n--- bus {bus} (initial {initial}/{maximum})")
    print("get --bus --brief  :", summary([get(bus)[2] for _ in range(10)]))

    for label, flags in (("set --noverify", ("--noverify",)), ("set (verify)", ())):
        times, failures = [], 0
        for i in range(10):
            code, out, ms = set_(bus, other if i % 2 == 0 else initial, *flags)
            times.append(ms)
            failures += code != 0
        print(f"{label:19}:", summary(times), f"échecs {failures}")

    set_(bus, other, "--noverify")
    value = get(bus)[0]
    print(f"relu juste après set --noverify : {value} (attendu {other})")

    # Rafale : ce que produirait un glisser sans coalescence.
    t = time.perf_counter()
    failures = 0
    for i in range(20):
        failures += set_(bus, other if i % 2 == 0 else initial, "--noverify")[0] != 0
    total = (time.perf_counter() - t) * 1000
    print(f"rafale 20 set      : {total:.0f} ms, {total / 20:.0f} ms/set, échecs {failures}, fin {get(bus)[0]} (attendu {initial})")


def in_parallel(jobs):
    results = [None] * len(jobs)

    def run(i, job):
        results[i] = job()

    threads = [threading.Thread(target=run, args=(i, j)) for i, j in enumerate(jobs)]
    t = time.perf_counter()
    for th in threads:
        th.start()
    for th in threads:
        th.join()
    return results, (time.perf_counter() - t) * 1000


def main():
    displays, detect_ms = detect()
    print(f"detect : {detect_ms:.0f} ms, {displays}")
    initial = {bus: get(bus)[:2] for bus in displays}
    print("valeurs initiales :", initial)
    if any(v is None for v, _ in initial.values()):
        raise SystemExit("lecture impossible, abandon avant toute écriture")

    try:
        first = min(displays)
        print("démarrage (--version):", summary([ddc("--version")[2] for _ in range(10)]))
        value = initial[first][0]
        other = value - 1 if value > 0 else value + 1
        for multiplier in ("1", "0.5", "0.2", "0.1"):
            opts = ("--disable-dynamic-sleep", "--sleep-multiplier", multiplier)
            gets = [ddc(*opts, "--bus", str(first), "--brief", "getvcp", "10") for _ in range(10)]
            sets = [set_(first, other if i % 2 == 0 else value, "--noverify", *opts) for i in range(10)]
            print(
                f"multiplicateur {multiplier:4}: get {summary([g[2] for g in gets])} échecs {sum(g[0] != 0 for g in gets)}"
                f" | set {summary([s[2] for s in sets])} échecs {sum(s[0] != 0 for s in sets)}, relu {get(first)[0]} (attendu {value})"
            )
        print("get --display 1    :", summary([ddc("--display", "1", "--brief", "getvcp", "10")[2] for _ in range(5)]))
        print("detect (répété)    :", summary([ddc("detect")[2] for _ in range(3)]))

        for bus, (value, maximum) in initial.items():
            write_tests(bus, value, maximum)

        print("\n--- concurrence")
        jobs = [lambda b=bus: set_(b, initial[b][0], "--noverify") for bus in displays]
        results, total = in_parallel(jobs)
        print(f"3 bus en parallèle : total {total:.0f} ms, individuels {[round(r[2]) for r in results]}, codes {[r[0] for r in results]}")

        value = initial[first][0]
        other = value - 1 if value > 0 else value + 1
        for attempt in range(5):
            results, total = in_parallel([lambda: set_(first, other, "--noverify"), lambda: set_(first, value, "--noverify")])
            print(f"même bus x2 #{attempt}     : total {total:.0f} ms, codes {[r[0] for r in results]}, {[r[1][:90] for r in results if r[0] != 0]}")
        results, total = in_parallel([lambda: get(first), lambda: set_(first, value, "--noverify")])
        print(f"get + set même bus : total {total:.0f} ms, lu {results[0][0]}, set code {results[1][0]}")

        print("\n--- cas d'erreur")
        cases = [
            ("bus inexistant", ("--bus", "99", "--brief", "getvcp", "10")),
            ("bus sans ACL (0)", ("--bus", "0", "--brief", "getvcp", "10")),
            ("bus sans écran (12)", ("--bus", "12", "--brief", "getvcp", "10")),
            ("code VCP non géré", ("--bus", str(first), "--brief", "getvcp", "E9")),
            # Ne pas ajouter de setvcp hors limites : ddcutil 3.0.2 a accepté 300 (code 0) et l'a envoyé à l'écran.
        ]
        for label, args in cases:
            code, out, ms = ddc(*args)
            print(f"{label:27}: code {code}, {ms:.0f} ms, {out[:130]!r}")
    finally:
        print("\n--- restauration")
        for bus, (value, _) in initial.items():
            set_(bus, value)
            print(f"bus {bus} : voulu {value}, relu {get(bus)[0]}")


if __name__ == "__main__":
    main()
