# homeostatic_forget_app.py
# PyQt6 + moderngl demo: Forgetful drawing with GPU density map and homeostatic feedback.
# Draw with mouse (left button). Press 'C' to clear. Use slider to set target density (0..1).

import sys, time, math
from dataclasses import dataclass
from typing import List, Tuple
import numpy as np
import json
import os
from datetime import datetime

from PyQt6.QtCore import Qt, QPointF, QTimer
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QSlider, QLabel, QWidget, QHBoxLayout, QVBoxLayout, QPushButton, QFileDialog, QStackedLayout
)
from PyQt6.QtGui import QSurfaceFormat, QPainter, QPen, QColor, QPainterPath, QIcon
from PyQt6.QtOpenGLWidgets import QOpenGLWidget
import moderngl


# ---- Utility: simple smoothing by midpoint quadratic conversion ----
def flatten_quadratic_bezier(p0, p1, p2, tol=0.5):
    pts = []

    def _flat(a, b, c):
        mx = (a[0] + c[0]) / 2.0
        my = (a[1] + c[1]) / 2.0
        bx = (a[0] + 2 * b[0] + c[0]) / 4.0
        by = (a[1] + 2 * b[1] + c[1]) / 4.0
        dx = bx - mx
        dy = by - my
        return dx * dx + dy * dy

    def _recurse(a, b, c):
        if _flat(a, b, c) <= tol * tol:
            pts.append(c)
            return
        ab = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0)
        bc = ((b[0] + c[0]) / 2.0, (b[1] + c[1]) / 2.0)
        abc = ((ab[0] + bc[0]) / 2.0, (ab[1] + bc[1]) / 2.0)
        _recurse(a, ab, abc)
        _recurse(abc, bc, c)

    pts.append(p0)
    _recurse(p0, p1, p2)
    return pts


# ---- Data model for strokes ----
@dataclass
class Stroke:
    points: List[Tuple[float, float, float]]  # (x, y, pressure)
    width: float
    color: Tuple[float, float, float]
    time_created: float
    base_alpha: float = 1.0  # dynamic alpha for forgetting
    is_visible: bool = True  # optimization flag
    _cached_path: QPainterPath = None

    def bbox(self):
        xs = [p[0] for p in self.points]
        ys = [p[1] for p in self.points]
        return min(xs), min(ys), max(xs), max(ys)


# ---- GL Canvas Widget ----
class GLCanvas(QOpenGLWidget):
    def __init__(self, density_w=128, density_h=96, parent=None):
        super().__init__(parent)
        self.setMouseTracking(True)

        self.strokes: List[Stroke] = []
        self.current_points = []
        self.density_w = density_w
        self.density_h = density_h

        # homeostasis parameters (現在無効化: 頂点数制限に移行)
        # self.target_density = 0.05
        # self.hysteresis = 0.02
        self.lambda_base = 0.2 # 薄くなる減衰率
        # self.lambda_k = 3.0
        self.lambdas_factor = 1.0
        self.max_display_points = 4000  # 描画をスムーズに保つための最大合計頂点数

        # moderngl objects
        self.ctx: moderngl.Context = None
        self.fbo = None
        self.density_tex = None
        self.prog = None

        # timer for repaint
        self.virtual_time = 0.0
        self.max_virtual_time = 0.0  # 最大値を記録
        self.last_virtual_time = 0.0 # 前回の時間を記録（巻き戻し検知用）
        self.timer = QTimer(self)
        self.timer.timeout.connect(self.advance_virtual_time)
        self.timeline_update_callback = None
        self.stop_play_callback = None  # 再生停止用コールバック

        # ハイライト用
        self.highlight_stroke_indices = []  # ハイライトするストロークリスト

    def set_highlight_stroke(self, indices):
        self.highlight_stroke_indices = indices
        self.update()
    
    def advance_virtual_time(self):
        self.virtual_time += 0.033  # 16msごとに加算
        if self.timeline_update_callback:
            self.timeline_update_callback(self.virtual_time)
        self.update()  

    def initializeGL(self):
        self.ctx = moderngl.create_context()
        # dtype="f1"をdtype="f4"に変更
        self.density_tex = self.ctx.texture(
            (self.density_w, self.density_h), components=4, dtype="f4"
        )
        self.density_tex.filter = (moderngl.LINEAR, moderngl.LINEAR)
        self.fbo = self.ctx.framebuffer(color_attachments=[self.density_tex])
        self.prog = self.ctx.program(
            vertex_shader="""
            #version 330
            in vec2 in_pos;
            void main() { gl_Position = vec4(in_pos, 0.0, 1.0); }
            """,
            fragment_shader="""
            #version 330
            out vec4 fcol;
            void main() { fcol = vec4(1.0,1.0,1.0,1.0); }
            """,
        )
        self.ctx.enable(moderngl.BLEND)
        self.ctx.blend_func = (moderngl.SRC_ALPHA, moderngl.ONE_MINUS_SRC_ALPHA)

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            x, y = e.position().x(), e.position().y()
            self.current_points = [(x, y, 1.0)]
            self.timer.start(33)
            # 再生中なら停止
            if self.stop_play_callback:
                self.stop_play_callback()

    def mouseMoveEvent(self, e):
        if e.buttons() & Qt.MouseButton.LeftButton:
            x, y = e.position().x(), e.position().y()
            self.current_points.append((x, y, 1.0))
            self.update()

    def mouseReleaseEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton and len(self.current_points) >= 2:
            s = Stroke(self.current_points.copy(), 6.0, (0, 0, 0), self.virtual_time)
            self.strokes.append(s)
            self.current_points = []
            # 最大virtual_timeを更新
            if self.virtual_time > self.max_virtual_time:
                self.max_virtual_time = self.virtual_time
            self.timer.stop()
            # タイムラインを再計算させる
            if hasattr(self, 'request_timeline_refresh'):
                self.request_timeline_refresh()

    def render_density_map(self):
        self.fbo.use()
        self.ctx.clear(0.0, 0.0, 0.0, 0.0)

        verts = []
        for s in self.strokes:
            if not s.is_visible:
                continue
            if s.time_created > self.virtual_time:
                continue
            if len(s.points) < 2:
                continue
            cw, ch = self.width(), self.height()
            for i in range(len(s.points) - 1):
                x0, y0, _ = s.points[i]
                x1, y1, _ = s.points[i + 1]
                dx, dy = x1 - x0, y1 - y0
                seg_len = math.hypot(dx, dy)
                if seg_len < 1e-6:
                    continue
                nx, ny = -dy / seg_len, dx / seg_len
                half_w = s.width / 2.0
                u0, v0 = x0 / cw, 1.0 - (y0 / ch)
                u1, v1 = x1 / cw, 1.0 - (y1 / ch)
                ndc0 = (u0 * 2 - 1, v0 * 2 - 1)
                ndc1 = (u1 * 2 - 1, v1 * 2 - 1)
                hwx = (half_w / cw) * 2
                hwy = (half_w / ch) * 2
                off = (nx * hwx, ny * hwy)
                v0a = (ndc0[0] + off[0], ndc0[1] + off[1])
                v0b = (ndc0[0] - off[0], ndc0[1] - off[1])
                v1a = (ndc1[0] + off[0], ndc1[1] + off[1])
                v1b = (ndc1[0] - off[0], ndc1[1] - off[1])
                verts += [
                    *v0a, *v1a, *v0b,
                    *v1a, *v1b, *v0b
                ]
        if not verts:
            return None

        vdata = np.array(verts, dtype="f4")
        vbo = self.ctx.buffer(vdata.tobytes())
        vao = self.ctx.simple_vertex_array(self.prog, vbo, "in_pos")
        vao.render(moderngl.TRIANGLES)
        vbo.release()
        vao.release()

        data = self.density_tex.read()
        arr = np.frombuffer(data, dtype=np.float32).reshape(
            (self.density_h, self.density_w, 4)
        )
        return np.clip(arr[..., 3], 0.0, 1.0)

    def paintEvent(self, event):
        # 巻き戻し検知: 時間が戻ったら、死んだストロークを復活させる可能性があるため全チェック
        if self.virtual_time < self.last_virtual_time:
            for s in self.strokes:
                s.is_visible = True

        painter = QPainter(self)
        painter.fillRect(self.rect(), QColor(255, 255, 255))
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        
        # 密度マップ計算をコメントアウト
        self.lambdas_factor = 1.0 # 固定の消滅ベクトル

        now = self.virtual_time
        
        # 頂点数による表示制限の計算 (新しいものから順にカウント)
        total_points = 0
        lam = self.lambda_base * self.lambdas_factor
        
        # ストロークを新しい順にチェックして表示可能か判断
        for s in reversed(self.strokes):
            # 未来のものはスキップ
            if s.time_created > now:
                s.is_visible = False
                continue
            
            # 時間による消滅計算
            age = now - s.time_created
            s.base_alpha = math.exp(-lam * age)
            
            # 完全に消えているか、頂点数制限を超えた場合は非表示
            if s.base_alpha < 0.01 or total_points > self.max_display_points:
                s.is_visible = False
            else:
                s.is_visible = True
                total_points += len(s.points)

        self.last_virtual_time = now

        pen = QPen(QColor(0, 0, 0))
        
        # 表示対象のみ描画
        for idx, s in enumerate(self.strokes):
            if not s.is_visible:
                continue
            
            if s._cached_path is None:
                path = QPainterPath()
                pts = s.points
                path.moveTo(QPointF(pts[0][0], pts[0][1]))
                for i in range(1, len(pts) - 1):
                    x1, y1, _ = pts[i]
                    x2, y2, _ = pts[i + 1]
                    cx, cy = (x1 + x2) / 2, (y1 + y2) / 2
                    path.quadTo(QPointF(x1, y1), QPointF(cx, cy))
                s._cached_path = path
            
            pen.setWidthF(s.width)
            # 段ハイライト中
            if idx in self.highlight_stroke_indices:
                c = QColor(0, 180, 255)
            else:
                c = QColor(0, 0, 0)
            
            alpha_val = max(0.0, min(1.0, s.base_alpha))
            c.setAlpha(int(255 * alpha_val))
            pen.setColor(c)
            painter.setPen(pen)
            painter.drawPath(s._cached_path)

        if self.current_points:
            path = QPainterPath()
            path.moveTo(QPointF(self.current_points[0][0], self.current_points[0][1]))
            for i in range(1, len(self.current_points) - 1):
                x1, y1, _ = self.current_points[i]
                x2, y2, _ = self.current_points[i + 1]
                cx, cy = (x1 + x2) / 2, (y1 + y2) / 2
                path.quadTo(QPointF(x1, y1), QPointF(cx, cy))
            pen.setWidthF(6.0)
            pen.setColor(QColor(0, 0, 0))
            painter.setPen(pen)
            painter.drawPath(path)
        painter.end()
        # print(f"[DEBUG] global_density={global_density:.4f}, lambda_factor={self.lambdas_factor:.3f}")

    def clear_all(self):
        self.strokes = []
        self.current_points = []
        self.virtual_time = 0.0
        self.max_virtual_time = 0.0
        self.last_virtual_time = 0.0

    def export_strokes_json(self):
        # strokesデータとvirtual_timeをJSON文字列で返す
        data = {
            "virtual_time": self.virtual_time,
            "strokes": [
                {
                    "points": s.points,
                    "width": s.width,
                    "color": s.color,
                    "time_created": s.time_created,
                    "base_alpha": s.base_alpha
                }
                for s in self.strokes
            ]
        }
        return json.dumps(data)

    def import_strokes_json(self, json_str):
        # JSON文字列からstrokesデータとvirtual_timeを復元
        data = json.loads(json_str)
        self.virtual_time = data.get("virtual_time", 0.0)
        self.max_virtual_time = max(self.virtual_time, max([s["time_created"] for s in data.get("strokes", [])], default=0.0))
        self.strokes = [
            Stroke(
                points=[tuple(p) for p in d["points"]],
                width=d["width"],
                color=tuple(d["color"]),
                time_created=d["time_created"],
                base_alpha=d.get("base_alpha", 1.0)
            )
            for d in data.get("strokes", [])
        ]
        self.update()


# ---- Timeline Widget ----
class TimelineWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.stroke_times = []
        self.timeline_max = 10.0
        self.current_time = 0.0
        self.highlight_segment = None
        self.segment_indices = []
        self.segment_ys = []
        self.setMinimumHeight(24)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setMouseTracking(True)  # マウストラッキング有効化
        self.is_dragging = False
        self.highlight_callback = None

        # カオスパッドモード用
        self.chaos_pad_mode = False  # スペースキー押下中かどうか
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)  # フォーカスを受け取るためにポリシーを設定

        self.line_height = 36 #世界線太さ
        self.density_data = []  # (time, count) list for graph
        self.max_display_points = 5000  # Default threshold
    def set_stroke_times(self, times, timeline_max, current_time=None):
        self.stroke_times = times
        self.timeline_max = timeline_max
        if current_time is not None:
            self.current_time = current_time
        self.calc_segments()
        self.update()

    def update_density_graph(self, strokes, timeline_max, lambda_base, max_display_points):
        # 頂点数の推移を計算（200地点サンプリング）
        self.max_display_points = max_display_points
        self.density_data = []
        if not strokes or timeline_max <= 0:
            return
        
        samples = 200
        for i in range(samples + 1):
            t = (i / samples) * timeline_max
            total_points = 0
            # GLCanvasの描画ロジックをシミュレート
            for s in reversed(strokes):
                if s.time_created > t:
                    continue
                age = t - s.time_created
                base_alpha = math.exp(-lambda_base * age)
                if base_alpha < 0.01 or total_points > max_display_points:
                    continue
                total_points += len(s.points)
            self.density_data.append((t, total_points))

    def calc_segments(self):
        # 段ごとのストロークインデックスリストとy座標リストを作成
        self.segment_indices = []
        self.segment_ys = []
        if not self.stroke_times:
            return
        y = 0
        indices = [0]
        prev_time = self.stroke_times[0]
        for i in range(1, len(self.stroke_times)):
            t = self.stroke_times[i]
            if t < prev_time:
                self.segment_indices.append(indices)
                self.segment_ys.append(y)
                y += self.line_height + 4
                indices = [i]
            else:
                indices.append(i)
            prev_time = t
        self.segment_indices.append(indices)
        self.segment_ys.append(y)

    def mousePressEvent(self, event):
        if self.chaos_pad_mode:
            self.handle_chaos_pad(event)
        else:
            self.is_dragging = True
            self.update_highlight_by_y(event.pos().y())

    def mouseMoveEvent(self, event):
        if self.chaos_pad_mode:
            self.handle_chaos_pad(event)
        else:
            self.update_highlight_by_y(event.pos().y())

    def mouseReleaseEvent(self, event):
        if self.chaos_pad_mode:
            # カオスパッドモード時は何もしない（仮想時間・世界線選択はmouseMoveEventでのみ反映）
            pass
        else:
            self.is_dragging = False
            self.highlight_segment = None
            if self.highlight_callback:
                self.highlight_callback([])
            self.update()

    def handle_chaos_pad(self, event):
        # Bキーを押している間だけ操作
        w = self.width()
        x = event.position().x() if hasattr(event, 'position') else event.x()
        y = event.position().y() if hasattr(event, 'position') else event.y()
        # 仮想時間（x座標）
        t = (x / w) * self.timeline_max if w > 0 else 0.0
        parent_canvas = self.parent().parent().parent().canvas
        parent_canvas.virtual_time = t
        self.current_time = t  # 赤いバーも動かす
        
        # MainWindowのUI更新（重い処理は避ける）
        if hasattr(parent_canvas, 'parent'):
            mainwin = self.parent().parent().parent()
            if hasattr(mainwin, 'update_timeline_slider'):
                mainwin.update_timeline_slider(t)
            # update_timeline_history (旧名) は重いので毎フレーム呼ばない
        
        parent_canvas.update()  # キャンバス描画を必ず更新
        # 世界線選択（y座標）
        self.update_highlight_by_y(y)

    def update_highlight_by_y(self, y):
        for seg_idx, seg_y in enumerate(self.segment_ys):
            if abs(y - (seg_y + self.line_height // 2)) < self.line_height // 2:
                if self.highlight_callback:
                    self.highlight_callback(self.segment_indices[seg_idx])
                self.highlight_segment = seg_idx
                self.update()
                return
        self.highlight_segment = None
        if self.highlight_callback:
            self.highlight_callback([])
        self.update()

    def set_chaos_pad_mode(self, enabled):
        self.chaos_pad_mode = enabled
        if enabled:
            self.setFocus()  # フォーカスを設定
        else:
            # モード終了時にハイライトをクリア
            if self.highlight_callback:
                self.highlight_callback([])
            self.highlight_segment = None
            self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setOpacity(0.5)
        w = self.width()
        h = self.height()
        painter.fillRect(self.rect(), QColor(255, 255, 255))

        # 頂点数グラフの描画
        if self.density_data:
            # 描画制限（max_display_points）が中央（高さ半分）に来るようにスケール設定
            graph_scale_max = self.max_display_points * 2
            
            painter.setOpacity(0.2)
            graph_path = QPainterPath()
            first = True
            for t, count in self.density_data:
                px = (t / self.timeline_max * w) if self.timeline_max > 0 else 0
                py = h - (count / graph_scale_max * h)
                # グラフが上端を突き抜けないようにクリップ
                py = max(0, py)
                if first:
                    graph_path.moveTo(px, py)
                    first = False
                else:
                    graph_path.lineTo(px, py)
            
            # グラフの下を塗りつぶし
            fill_path = QPainterPath(graph_path)
            fill_path.lineTo(w, h)
            fill_path.lineTo(0, h)
            fill_path.closeSubpath()
            painter.fillPath(fill_path, QColor(0, 120, 255))
            
            # 折れ線自体を描画
            painter.setOpacity(0.4)
            pen_graph = QPen(QColor(0, 80, 200), 1)
            painter.setPen(pen_graph)
            painter.drawPath(graph_path)

            # 閾値（描画制限）を表す緑色の線を描画
            painter.setOpacity(0.6)
            pen_threshold = QPen(QColor(0, 255, 0), 2)
            painter.setPen(pen_threshold)
            painter.drawLine(0, h // 2, w, h // 2)
            
            painter.setOpacity(0.5)

        # 世界線（段）ごとに描画
        for seg_idx, indices in enumerate(self.segment_indices):
            y = self.segment_ys[seg_idx]
            prev_time = self.stroke_times[indices[0]]
            start_x = int(prev_time / self.timeline_max * w) if self.timeline_max > 0 else 0
            for i in indices[1:]:
                t = self.stroke_times[i]
                x = int(t / self.timeline_max * w) if self.timeline_max > 0 else 0
                pen = QPen(QColor(0, 120, 255, 200), self.line_height)
                if self.highlight_segment == seg_idx:
                    pen = QPen(QColor(0, 200, 0, 220), self.line_height + 2)  # 緑色を太く濃く
                painter.setPen(pen)
                painter.drawLine(start_x, y + self.line_height // 2, x, y + self.line_height // 2)
                start_x = x
            # 段ハイライト用の薄い緑横線
            if self.highlight_segment == seg_idx:
                pen = QPen(QColor(0, 200, 0, 180), 4)
                painter.setPen(pen)
                painter.drawLine(0, y + self.line_height // 2, w, y + self.line_height // 2)
        # 現在選択中の時間を縦線で表示
        if self.timeline_max > 0:
            cx = int(self.current_time / self.timeline_max * w)
            pen_cursor = QPen(QColor(255, 0, 0, 180), 2)
            painter.setPen(pen_cursor)
            painter.drawLine(cx, 0, cx, h)
        painter.end()


# ---- Main Window ----
class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("Homeostatic Forget Drawing")
        self.resize(1600, 1000)

        # アイコンの設定
        icon_path = os.path.join(os.path.dirname(__file__), "Icon", "タイムリープメモ_アイコン.svg")
        if os.path.exists(icon_path):
            self.setWindowIcon(QIcon(icon_path))
        
        self.timeline_max = 300.0  # 仮想時刻の最大値（秒）
        self.chaos_pad_mode = False

        self.canvas = GLCanvas()
        self.timeline_widget = TimelineWidget(self.canvas)
        self.timeline_widget.set_stroke_times(self.get_stroke_times(), self.timeline_max)
        self.timeline_widget.hide()

        self.timeline_slider = QSlider(Qt.Orientation.Horizontal)
        self.timeline_slider.setRange(0, 100)
        self.timeline_slider.setValue(0)
        self.timeline_slider.valueChanged.connect(self.on_timeline_slider)

        self.play_button = QPushButton("▶")
        self.play_button.setCheckable(True)
        self.play_button.clicked.connect(self.on_play_clicked)

        self.label = QLabel("Time: 0.00")

        self.export_button = QPushButton("エクスポート")
        self.export_button.clicked.connect(self.export_strokes)
        self.import_button = QPushButton("インポート")
        self.import_button.clicked.connect(self.import_strokes)
        self.now_button = QPushButton("now")
        self.now_button.clicked.connect(self.go_to_now)

        # キャンバスとタイムラインウィジェットを重ねる
        stack_widget = QWidget()
        stack_layout = QStackedLayout(stack_widget)
        stack_layout.addWidget(self.canvas)
        stack_layout.addWidget(self.timeline_widget)
        stack_layout.setStackingMode(QStackedLayout.StackingMode.StackAll)
        self.timeline_widget.raise_()  # ウィジェットを前面に
        self.timeline_widget.hide() 

        layout = QVBoxLayout()
        layout.addWidget(stack_widget)
        hl = QHBoxLayout()
        hl.addWidget(self.label)
        hl.addWidget(self.timeline_slider)
        hl.addWidget(self.play_button)
        hl.addWidget(self.export_button)
        hl.addWidget(self.import_button)
        hl.addWidget(self.now_button)
        layout.addLayout(hl)
        w = QWidget()
        w.setLayout(layout)
        self.setCentralWidget(w)

        # 再生用タイマー
        self.play_timer = QTimer(self)
        self.play_timer.timeout.connect(self.advance_timeline)
        

        self.canvas.timeline_update_callback = self.update_timeline_slider
        self.timeline_widget.highlight_callback = self.canvas.set_highlight_stroke
        self.canvas.request_timeline_refresh = self.refresh_timeline_data

        # 設定ファイルをAppDataに保存
        # AppData\Local\TimeLeapMemo\config.json
        appdata_dir = os.path.join(os.getenv('LOCALAPPDATA'), 'TimeLeapMemo')
        # フォルダが存在しない場合は作成
        if not os.path.exists(appdata_dir):
            os.makedirs(appdata_dir)
        self.config_path = os.path.join(appdata_dir, "config.json")
        # 初回起動時にデフォルトのconfig.jsonを作成
        if not os.path.exists(self.config_path):
            self.save_last_folder("C:/")
        self.last_folder = self.load_last_folder()

        self.timeline_slider.enterEvent = self.timeline_slider_enter
        self.timeline_slider.leaveEvent = self.timeline_slider_leave

    def load_last_folder(self):
        if os.path.exists(self.config_path):
            try:
                with open(self.config_path, "r", encoding="utf-8") as f:
                    conf = json.load(f)
                return conf.get("last_folder", "")
            except Exception:
                return ""
        return ""

    def save_last_folder(self, folder):
        try:
            with open(self.config_path, "w", encoding="utf-8") as f:
                json.dump({"last_folder": folder}, f)
        except Exception:
            pass

    def get_stroke_times(self):
        return [s.time_created for s in self.canvas.strokes]

    def refresh_timeline_data(self):
        self.timeline_widget.set_stroke_times(self.get_stroke_times(), self.timeline_max, self.canvas.virtual_time)
        self.timeline_widget.update_density_graph(
            self.canvas.strokes, self.timeline_max, self.canvas.lambda_base, self.canvas.max_display_points
        )

    def update_timeline_slider(self, t):
        slider_val = int(t / self.timeline_max * 100)
        self.timeline_slider.blockSignals(True)
        self.timeline_slider.setValue(slider_val)
        self.timeline_slider.blockSignals(False)
        self.label.setText(f"Time: {t:.2f}")
        self.timeline_widget.current_time = t
        self.timeline_widget.update()

    def on_timeline_slider(self, v):
        t = v / 100.0 * self.timeline_max
        self.canvas.virtual_time = t
        self.label.setText(f"Time: {t:.2f}")
        self.timeline_widget.current_time = t # 赤いバーを更新
        self.timeline_widget.update()
        self.canvas.update()

    def on_play_clicked(self, checked):
        if checked:
            self.play_button.setText("⏸")
            self.play_timer.start(33)
        else:
            self.play_button.setText("▶")
            self.play_timer.stop()

    def advance_timeline(self):
        t = self.canvas.virtual_time + 0.033
        # 最大virtual_timeに到達したら自動停止
        if t >= self.canvas.max_virtual_time:
            t = self.canvas.max_virtual_time
            self.play_button.setChecked(False)
            self.play_button.setText("▶")
            self.play_timer.stop()
        self.canvas.virtual_time = t
        slider_val = int(t / self.timeline_max * self.timeline_slider.maximum())
        self.timeline_slider.blockSignals(True)
        self.timeline_slider.setValue(slider_val)
        self.timeline_slider.blockSignals(False)
        self.label.setText(f"Time: {t:.2f}")
        self.canvas.update()

    def keyPressEvent(self, e):
        if self.chaos_pad_mode:
            # カオスパッドモード中はBキー以外の処理を完全に無効化
            if e.key() == Qt.Key.Key_B:
                self.chaos_pad_mode = True
                self.timeline_widget.chaos_pad_mode = True
                self.timeline_widget.show()
                self.timeline_widget.raise_()
                self.timeline_widget.resize(self.canvas.size())
                self.timeline_slider.setEnabled(False)
            return
        if e.key() == Qt.Key.Key_C:
            self.canvas.clear_all()
            self.refresh_timeline_data()
        elif e.key() == Qt.Key.Key_N:
            self.go_to_now()
        elif e.key() == Qt.Key.Key_B:
            self.chaos_pad_mode = True
            self.timeline_widget.chaos_pad_mode = True
            self.timeline_widget.show()
            self.timeline_widget.raise_()
            self.timeline_widget.resize(self.canvas.size())
            self.timeline_slider.setEnabled(False)

    def keyReleaseEvent(self, e):
        if e.key() == Qt.Key.Key_B:
            self.chaos_pad_mode = False
            self.timeline_widget.chaos_pad_mode = False
            self.timeline_widget.hide()
            self.timeline_slider.setEnabled(True)

    def stop_play(self):
        self.play_timer.stop()
        self.play_button.setChecked(False)
        self.play_button.setText("▶")

    def export_strokes(self):
        timestamp = datetime.now().strftime("%Y_%m_%d_%H%M%S")
        default_name = f"{timestamp}.json"
        path, _ = QFileDialog.getSaveFileName(self, "エクスポート", os.path.join(self.last_folder, default_name), "JSON Files (*.json)")
        if path:
            folder = os.path.dirname(path)
            self.save_last_folder(folder)
            json_str = self.canvas.export_strokes_json()
            with open(path, "w", encoding="utf-8") as f:
                f.write(json_str)
            self.refresh_timeline_data()

    def import_strokes(self):
        path, _ = QFileDialog.getOpenFileName(self, "インポート", self.last_folder, "JSON Files (*.json)")
        if path:
            folder = os.path.dirname(path)
            self.save_last_folder(folder)
            with open(path, "r", encoding="utf-8") as f:
                json_str = f.read()
            self.canvas.import_strokes_json(json_str)
            # 仮想時間に合わせてタイムラインバーも更新
            t = self.canvas.virtual_time
            slider_val = int(t / self.timeline_max * self.timeline_slider.maximum())
            self.timeline_slider.blockSignals(True)
            self.timeline_slider.setValue(slider_val)
            self.timeline_slider.blockSignals(False)
            self.label.setText(f"Time: {t:.2f}")
            self.refresh_timeline_data()

    def go_to_now(self):
        t = self.canvas.max_virtual_time
        self.canvas.virtual_time = t
        slider_val = int(t / self.timeline_max * self.timeline_slider.maximum())
        self.timeline_slider.blockSignals(True)
        self.timeline_slider.setValue(slider_val)
        self.timeline_slider.blockSignals(False)
        self.label.setText(f"Time: {t:.2f}")
        self.canvas.update()
        self.refresh_timeline_data()

    def timeline_slider_enter(self, event):
        self.timeline_widget.show()

    def timeline_slider_leave(self, event):
        self.timeline_widget.hide()


# ---- Run ----
if __name__ == "__main__":
    # Windowsのタスクバーでアイコンを正しく表示するための設定
    if sys.platform == 'win32':
        import ctypes
        myappid = 'keeeewai-kanikani.timeleapmemo.v1.0'
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(myappid)

    app = QApplication(sys.argv)
    fmt = QSurfaceFormat()
    fmt.setDepthBufferSize(24)
    QSurfaceFormat.setDefaultFormat(fmt)
    w = MainWindow()
    w.show()
    sys.exit(app.exec())