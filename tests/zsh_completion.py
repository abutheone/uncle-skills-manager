"""Exercise real interactive ZLE completion, including a newly created skill."""
import fcntl
import os
from pathlib import Path
import pty
import select
import shlex
import subprocess
import sys
import tempfile
import termios
import time

binary = Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix="uncle-zsh-") as temporary:
    root = Path(temporary)
    repo = root / "source"
    (repo / "skills").mkdir(parents=True)
    config = root / "config.toml"
    def skill(name, marker):
        folder = repo / "skills" / name
        folder.mkdir()
        (folder / "SKILL.md").write_text(
            f'---\nname: {name}\ndescription: "Write a test result."\n---\n{marker}\n'
        )
    skill("live-first", "FIRST_CONTENT")
    subprocess.run([str(binary), "--config", str(config), "init", "--repo", str(repo),
                    "--target", "codex=" + str(root / "agent"), "--method", "copy"],
                   check=True, capture_output=True)
    master, slave = pty.openpty()
    def terminal():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    env = dict(os.environ, HOME=str(root), ZDOTDIR=str(root), TERM="xterm",
               UNCLE_SKILLS_CONFIG=str(config),
               PATH=str(binary.parent) + os.pathsep + os.environ["PATH"])
    process = subprocess.Popen(["zsh", "-f"], stdin=slave, stdout=slave, stderr=slave,
                               env=env, preexec_fn=terminal, close_fds=True)
    os.close(slave)
    def send(value):
        os.write(master, value.encode())
    def expect(text):
        collected = b""
        end = time.monotonic() + 20
        while time.monotonic() < end:
            ready, _, _ = select.select([master], [], [], max(0, end-time.monotonic()))
            if not ready:
                break
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            collected += data
            if text.encode() in collected:
                return collected
        raise AssertionError(f"Expected {text!r}, received {collected!r}")
    try:
        # Use printf octal escapes so the ready marker occurs in output, not input echo.
        send("autoload -Uz compinit; compinit -D; source <(uncle-skills completions zsh); printf '\\122\\105\\101\\104\\131\\012'\n")
        expect("READY\r\n")
        send("uncle-skills show live-f\t\n")
        expect("FIRST_CONTENT")
        skill("live-second", "SECOND_CONTENT")
        send("uncle-skills show live-s\t\n")
        expect("SECOND_CONTENT")
        # Complete a subcommand and then one of its flags in the actual shell.
        send("uncle-skills sy\t --dry-r\t\n")
        expect("Install")
        assert not (root / "agent").exists(), "Completion dry-run wrote agent files"
        print("Interactive zsh: subcommands, flags, and fresh skill names passed")
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        os.close(master)
