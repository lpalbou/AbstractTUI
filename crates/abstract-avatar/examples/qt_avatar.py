"""Optional Qt host for the shared native renderer; no Gateway or inference.

Run: python qt_avatar.py /path/to/libabstract_avatar.dylib
PyQt5 is an example-only dependency. Rendering and pointer events stay on Qt's
GUI thread. A parent app can forward pointer movement through look_at(x, y).
"""
import ctypes as C
import sys
import time

from PyQt5.QtCore import QTimer, Qt
from PyQt5.QtGui import QImage, QPainter
from PyQt5.QtWidgets import QApplication, QWidget, QComboBox, QVBoxLayout, QHBoxLayout, QLabel


class AvatarWidget(QWidget):
    def __init__(self, library, parent=None):
        super().__init__(parent)
        self.lib = library
        self.handle = library.avatar_create(192, 192)
        if not self.handle:
            raise RuntimeError("Avatar renderer capacity exceeded")
        self.design, self.pose = 0, 3
        self.animated = True
        self.gaze = (0.0, 0.0)
        self.start = time.monotonic()
        self.image = QImage()
        self.setMinimumSize(220, 220)
        self.setMouseTracking(True)
        palette = self.palette()
        background = palette.window().color()
        foreground = palette.windowText().color()
        for material, mid in enumerate([(211, 158, 57), (225, 70, 95), (80, 90, 105)]):
            high = foreground if material < 2 else palette.mid().color()
            for i in range(256):
                t = i / 255
                a, b, u = ((background.red(), background.green(), background.blue()), mid, t / .65) if t < .65 else (mid, (high.red(), high.green(), high.blue()), (t - .65) / .35)
                rgb = [int(left + (right - left) * u + .5) for left, right in zip(a, b)]
                self.lib.avatar_set_shade(self.handle, material, i, (rgb[0] << 24) | (rgb[1] << 16) | (rgb[2] << 8) | 255)
        self.timer = QTimer(self)
        self.timer.timeout.connect(self.frame)
        self.idle = QTimer(self)
        self.idle.setSingleShot(True)
        self.idle.timeout.connect(self.reset_gaze)

    def frame(self):
        if not self.handle:
            return
        self.lib.avatar_set_gaze(self.handle, *self.gaze)
        code = self.lib.avatar_render(self.handle, self.design, self.pose, .6, time.monotonic() - self.start if self.animated else 0, 1)
        if code:
            raise RuntimeError(f"Avatar render refused: {code}")
        data = C.string_at(self.lib.avatar_pixels(self.handle), self.lib.avatar_pixels_len(self.handle))
        self.image = QImage(data, 192, 192, 192 * 4, QImage.Format_RGBA8888).copy()
        self.update()

    def schedule(self):
        self.timer.stop()
        if self.animated and self.isVisible() and self.pose != 6:
            period = 84 if self.idle.isActive() else 500 if self.pose == 0 else 167 if self.pose == 5 else 84
            self.timer.start(period)

    def reset_gaze(self):
        self.idle.stop()
        self.gaze = (0.0, 0.0)
        self.frame()
        self.schedule()

    def look_at(self, x, y):
        """Normalized cursor target, -1..1. Feed actual motion events, never poll."""
        if self.pose != 0 or not self.animated or not self.isVisible():
            return
        tracking = self.idle.isActive()
        self.gaze = (max(-1, min(1, x)), max(-1, min(1, y)))
        self.idle.start(2000)
        if not tracking:
            self.frame()
            self.schedule()

    def mouseMoveEvent(self, event):
        side = max(1, min(self.width(), self.height()))
        self.look_at((event.x() - self.width() / 2) / (side / 2), (event.y() - self.height() / 2) / (side / 2))
        super().mouseMoveEvent(event)

    def activity(self, design, pose):
        self.design, self.pose = design, pose
        if pose != 0:
            self.idle.stop()
            self.gaze = (0.0, 0.0)
        self.frame()
        self.schedule()

    def set_animated(self, enabled):
        self.animated = bool(enabled)
        if not enabled:
            self.idle.stop()
            self.gaze = (0.0, 0.0)
        self.frame()
        self.schedule()

    def showEvent(self, event):
        super().showEvent(event)
        self.activity(self.design, self.pose)

    def hideEvent(self, event):
        self.timer.stop()
        self.idle.stop()
        self.gaze = (0.0, 0.0)
        super().hideEvent(event)

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.SmoothPixmapTransform)
        side = min(self.width(), self.height())
        painter.drawImage((self.width() - side) // 2, (self.height() - side) // 2, self.image.scaled(side, side, Qt.KeepAspectRatio, Qt.SmoothTransformation))

    def release(self):
        self.timer.stop()
        self.idle.stop()
        if self.handle:
            self.lib.avatar_destroy(self.handle)
            self.handle = 0

    def closeEvent(self, event):
        self.release()
        super().closeEvent(event)


def load_library(path):
    lib = C.CDLL(path)
    lib.avatar_create.argtypes = [C.c_uint32, C.c_uint32]
    lib.avatar_create.restype = C.c_uint32
    lib.avatar_set_shade.argtypes = [C.c_uint32] * 4
    lib.avatar_set_gaze.argtypes = [C.c_uint32, C.c_float, C.c_float]
    lib.avatar_render.argtypes = [C.c_uint32] * 3 + [C.c_float, C.c_float, C.c_uint32]
    lib.avatar_pixels.argtypes = [C.c_uint32]
    lib.avatar_pixels.restype = C.c_void_p
    lib.avatar_pixels_len.argtypes = [C.c_uint32]
    lib.avatar_pixels_len.restype = C.c_size_t
    lib.avatar_style_field_ptr.argtypes = [C.c_uint32, C.c_uint32]
    lib.avatar_style_field_ptr.restype = C.c_void_p
    lib.avatar_style_field_len.argtypes = [C.c_uint32, C.c_uint32]
    lib.avatar_style_field_len.restype = C.c_size_t
    lib.avatar_destroy.argtypes = [C.c_uint32]
    lib.avatar_style_count.restype = C.c_uint32
    if lib.avatar_abi_version() != 1:
        raise RuntimeError("Unsupported shared avatar ABI")
    return lib


def main():
    lib = load_library(sys.argv[1])
    app = QApplication(sys.argv)
    window = QWidget()
    layout = QVBoxLayout(window)
    controls = QHBoxLayout()
    design, pose = QComboBox(), QComboBox()
    for i in range(lib.avatar_style_count()):
        design.addItem(C.string_at(lib.avatar_style_field_ptr(i, 1), lib.avatar_style_field_len(i, 1)).decode())
    pose.addItems(["Ready", "Thinking", "Working", "Listening", "Speaking", "Waiting", "Error"])
    pose.setCurrentIndex(3)
    controls.addWidget(QLabel("Design"))
    controls.addWidget(design)
    controls.addWidget(QLabel("Phase"))
    controls.addWidget(pose)
    layout.addLayout(controls)
    avatar = AvatarWidget(lib)
    layout.addWidget(avatar)
    design.currentIndexChanged.connect(lambda _: avatar.activity(design.currentIndex(), pose.currentIndex()))
    pose.currentIndexChanged.connect(lambda _: avatar.activity(design.currentIndex(), pose.currentIndex()))
    app.aboutToQuit.connect(avatar.release)
    window.setWindowTitle("Shared avatar · Qt")
    window.resize(420, 400)
    window.show()
    app.exec_()


if __name__ == "__main__":
    main()
