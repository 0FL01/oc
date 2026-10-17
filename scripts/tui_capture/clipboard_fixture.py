"""Private headless Wayland clipboard; never connects to the owner's desktop."""
import atexit
import base64
import os
from pathlib import Path
import select
import signal
import subprocess
import tempfile
import time

PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgZGL+DwABFAEG1rmmRQAAAABJRU5ErkJggg=='


class Clipboard:
    def __init__(self, env):
        self.root = tempfile.TemporaryDirectory(prefix='capture-clip-', dir='/tmp/opencode')
        self.processes = []
        atexit.register(self.close)
        env.update(XDG_RUNTIME_DIR=self.root.name, WAYLAND_DISPLAY='capture-clipboard',
                   QT_QPA_PLATFORM='offscreen', KWIN_COMPOSE='Q', LIBGL_ALWAYS_SOFTWARE='1')
        self.processes.append(subprocess.Popen([
            '/usr/bin/dbus-run-session', '--', '/usr/bin/kwin_wayland', '--virtual',
            '--width', '640', '--height', '480', '--no-lockscreen', '--no-global-shortcuts',
            '--no-kactivities', '--socket', 'capture-clipboard'], env=env,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True))
        deadline = time.monotonic() + 8
        while not Path(self.root.name, 'capture-clipboard').exists():
            if self.processes[0].poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError('private clipboard compositor failed to become ready')
            select.select([], [], [], .01)
        owner = subprocess.Popen(['/usr/bin/wl-copy', '--foreground', '--type', 'image/png'],
                                 env=env, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL, start_new_session=True)
        self.processes.append(owner)
        owner.stdin.write(base64.b64decode(PNG))
        owner.stdin.close()
        while time.monotonic() < deadline:
            result = subprocess.run(['/usr/bin/wl-paste', '--no-newline', '--type', 'image/png'],
                                    env=env, capture_output=True, timeout=1)
            if result.returncode == 0 and result.stdout == base64.b64decode(PNG):
                return
            if owner.poll() is not None:
                break
            select.select([], [], [], .01)
        raise RuntimeError('private PNG clipboard owner failed to become ready')

    def close(self):
        for process in reversed(self.processes):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
        self.processes.clear()
        self.root.cleanup()


def verify(body):
    images = [part.get('image_url') for item in body.get('input', [])
              for part in item.get('content', []) if part.get('type') == 'input_image']
    return images == ['data:image/png;base64,' + PNG]
