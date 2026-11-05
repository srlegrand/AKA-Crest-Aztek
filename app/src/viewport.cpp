#include "viewport.h"
#include "aztek.h"
#include "host.h"
#include "icons.h"
#include "panels.h"
#include <QJsonArray>

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QGridLayout>
#include <QGuiApplication>
#include <QMenuBar>
#include <QMouseEvent>
#include <QToolBar>
#include <QVBoxLayout>
#include <QWheelEvent>
#include <QWindow>
#include <cmath>
#include <QtGui/qguiapplication_platform.h>

#ifdef Q_OS_WIN
#include <windows.h>
#endif

namespace az {

namespace {
int mods_of(Qt::KeyboardModifiers m) {
    int r = 0;
    if (m & Qt::ShiftModifier) r |= 1;
    if (m & Qt::ControlModifier) r |= 2;
    if (m & Qt::AltModifier) r |= 4;
    return r;
}
int buttons_of(Qt::MouseButtons b) {
    int r = 0;
    if (b & Qt::LeftButton) r |= 1;
    if (b & Qt::MiddleButton) r |= 2;
    if (b & Qt::RightButton) r |= 4;
    return r;
}
int button_of(Qt::MouseButton b) {
    switch (b) {
    case Qt::LeftButton: return 1;
    case Qt::MiddleButton: return 2;
    case Qt::RightButton: return 3;
    default: return 0;
    }
}
QList<ViewportWidget*>& all_views() {
    static QList<ViewportWidget*> v;
    return v;
}
} // namespace

ViewportWidget::ViewportWidget(const QString& camera, QWidget* parent) : QWidget(parent), camera_(camera) {
    setAttribute(Qt::WA_NativeWindow);
    setAttribute(Qt::WA_PaintOnScreen);
    setAttribute(Qt::WA_NoSystemBackground);
    setAttribute(Qt::WA_OpaquePaintEvent);
    setMouseTracking(true);
    setFocusPolicy(Qt::StrongFocus);
    setMinimumSize(64, 64);
    all_views().append(this);
    connect(Host::get(), &Host::sceneChanged, this, [this] { update(); });
}

ViewportWidget::~ViewportWidget() {
    all_views().removeAll(this);
    if (id_) aztek_view_destroy(id_);
}

void ViewportWidget::ensure() {
    if (id_ || !isVisible()) return;
    double dpr = devicePixelRatioF();
    int w = int(width() * dpr), h = int(height() * dpr);
    QByteArray cam = camera_.toUtf8();
#if defined(Q_OS_WIN)
    id_ = aztek_view_create(AZTEK_WIN32, nullptr, quint64(winId()), (void*)GetModuleHandleW(nullptr), w, h, dpr, cam.constData());
#elif defined(Q_OS_LINUX)
    if (auto* x11 = qApp->nativeInterface<QNativeInterface::QX11Application>()) {
        id_ = aztek_view_create(AZTEK_XCB, (void*)x11->connection(), quint64(winId()), nullptr, w, h, dpr, cam.constData());
    } else {
        log("// Error: viewport: run on X11 (QT_QPA_PLATFORM=xcb) //", 2);
    }
#endif
    if (id_) update();
}

void ViewportWidget::showEvent(QShowEvent*) {
    ensure();
}

void ViewportWidget::paintEvent(QPaintEvent*) {
    ensure();
    if (id_) aztek_view_render(id_);
}

void ViewportWidget::resizeEvent(QResizeEvent*) {
    if (!id_) return;
    double dpr = devicePixelRatioF();
    aztek_view_resize(id_, int(width() * dpr), int(height() * dpr));
    update();
}

bool ViewportWidget::event(QEvent* e) {
    return QWidget::event(e);
}

void ViewportWidget::mouse(int kind, QMouseEvent* e) {
    if (!id_) return;
    double dpr = devicePixelRatioF();
    QPointF p = e->position() * dpr;
    int r = aztek_view_mouse(id_, kind, kind == 1 ? 0 : button_of(e->button()), p.x(), p.y(), mods_of(e->modifiers()), buttons_of(e->buttons()));
    if (r) update();
}

void ViewportWidget::mousePressEvent(QMouseEvent* e) {
    setFocus();
    emit activated(this);
    // Right button without Alt opens the marking menu; Alt+right dollies.
    if (e->button() == Qt::RightButton && !(e->modifiers() & Qt::AltModifier)) {
        emit rightClick(e->globalPosition().toPoint(), mods_of(e->modifiers()));
        return;
    }
    if (e->button() == Qt::MiddleButton && !(e->modifiers() & Qt::AltModifier) && !selected_channels().isEmpty()) {
        QJsonArray sel = state()["selection"].toArray();
        slide_plugs_.clear();
        for (const QJsonValue& o : sel)
            for (const QString& a : selected_channels()) {
                QString plug = o.toString().section('.', 0, 0) + "." + a;
                QJsonValue v = ask("getAttr", QJsonObject{{"args", QJsonArray{plug}}});
                if (v.isDouble()) slide_plugs_.append({plug, v.toDouble()});
            }
        if (!slide_plugs_.isEmpty()) {
            sliding_ = true;
            slide_start_ = e->position();
            slide_applied_ = 0;
            return;
        }
    }
    mouse(0, e);
}

void ViewportWidget::slide_to(double dx, bool final) {
    for (int i = 0; i < slide_applied_; ++i) aztek_retract();
    slide_applied_ = 0;
    // A tenth per pixel; Ctrl for fine steps, Shift for coarse, as Maya's slider.
    double step = (QGuiApplication::keyboardModifiers() & Qt::ControlModifier) ? 0.01 : (QGuiApplication::keyboardModifiers() & Qt::ShiftModifier) ? 1.0 : 0.1;
    if (dx == 0) {
        Host::get()->poke();
        return;
    }
    for (auto& [plug, start] : slide_plugs_) {
        QJsonObject call{{"args", QJsonArray{plug, start + dx * step}}};
        QJsonValue r = final ? exec("setAttr", call) : exec_quiet("setAttr", call);
        if (!final && !r.isUndefined()) ++slide_applied_;
    }
    Host::get()->poke();
    update();
}

void ViewportWidget::mouseMoveEvent(QMouseEvent* e) {
    if (sliding_) {
        slide_to(std::round(e->position().x() - slide_start_.x()), false);
        return;
    }
    mouse(1, e);
}
void ViewportWidget::mouseReleaseEvent(QMouseEvent* e) {
    if (sliding_ && e->button() == Qt::MiddleButton) {
        sliding_ = false;
        slide_to(std::round(e->position().x() - slide_start_.x()), true);
        return;
    }
    if (e->button() == Qt::RightButton && !(e->modifiers() & Qt::AltModifier)) return;
    mouse(2, e);
    Host::get()->poke();
}

void ViewportWidget::mouseDoubleClickEvent(QMouseEvent* e) {
    if (e->modifiers() & Qt::AltModifier) return;
    mouse(3, e);
    Host::get()->poke();
}

void ViewportWidget::leaveEvent(QEvent*) {
    // Drop the pre-selection highlight: a move far outside the view picks nothing.
    if (id_ && aztek_view_mouse(id_, 1, 0, -1e6, -1e6, 0, 0)) update();
}

void ViewportWidget::wheelEvent(QWheelEvent* e) {
    if (!id_) return;
    aztek_view_wheel(id_, e->angleDelta().y() / 8.0);
    update();
}

void ViewportWidget::set(const QString& key, const QString& json) {
    if (!id_) ensure();
    if (!id_) return;
    aztek_view_set(id_, key.toUtf8().constData(), json.toUtf8().constData());
    update();
}

QString ViewportWidget::get(const QString& key) const {
    if (!id_) return QString();
    return take(aztek_view_get(id_, key.toUtf8().constData()));
}

bool ViewportWidget::capture(const QString& path) {
    ensure();
    return id_ && aztek_view_capture(id_, path.toUtf8().constData());
}

// ---------------------------------------------------------------------------------------------------------------------

ModelPanel::ModelPanel(const QString& camera, QWidget* parent) : QWidget(parent) {
    auto* lay = new QVBoxLayout(this);
    lay->setContentsMargins(0, 0, 0, 0);
    lay->setSpacing(0);
    menus_ = new QMenuBar(this);
    menus_->setNativeMenuBar(false);
    menus_->setObjectName("panelMenus");
    bar_ = new QToolBar(this);
    bar_->setObjectName("panelBar");
    bar_->setIconSize(QSize(18, 18));
    view_ = new ViewportWidget(camera, this);
    lay->addWidget(menus_);
    lay->addWidget(bar_);
    lay->addWidget(view_, 1);
    build_menus();
}

void ModelPanel::build_menus() {
    auto* v = view_;
    auto set = [v](const char* k, const QString& j) { return [v, k, j] { v->set(k, j); }; };

    QMenu* view = menus_->addMenu("View");
    for (const char* c : {"persp", "top", "front", "side"}) {
        QString cam = c;
        view->addAction(QString("Camera: %1").arg(cam), [v, cam] { v->set("camera", "\"" + cam + "\""); });
    }
    view->addSeparator();
    view->addAction("Frame All\tA", set("frame", "\"all\""));
    view->addAction("Frame Selection\tF", set("frame", "\"selection\""));
    view->addAction("Default Home", set("home", "true"));

    QMenu* shading = menus_->addMenu("Shading");
    auto* group = new QActionGroup(this);
    QAction* wire = shading->addAction("Wireframe\t4", set("wireframe", "true"));
    QAction* smooth = shading->addAction("Smooth Shade All\t5", set("wireframe", "false"));
    wire->setCheckable(true);
    smooth->setCheckable(true);
    smooth->setChecked(true);
    group->addAction(wire);
    group->addAction(smooth);
    shading->addSeparator();
    QAction* wos = shading->addAction("Wireframe on Shaded");
    wos->setCheckable(true);
    connect(wos, &QAction::toggled, this, [v](bool on) { v->set("wireOnShaded", on ? "true" : "false"); });

    menus_->addMenu("Lighting")->addAction("Use Default Lighting")->setEnabled(false);

    QMenu* show = menus_->addMenu("Show");
    QAction* grid = show->addAction("Grid");
    grid->setCheckable(true);
    grid->setChecked(true);
    connect(grid, &QAction::toggled, this, [v](bool on) { v->set("grid", on ? "true" : "false"); });

    QMenu* panels = menus_->addMenu("Panels");
    panels->addAction("Single Pane / Four Panes\tSpace", [this] {
        for (QWidget* w = parentWidget(); w; w = w->parentWidget())
            if (auto* pl = qobject_cast<PanelLayout*>(w)) {
                pl->toggle();
                break;
            }
    });

    bar_->addAction(icon("frameAll"), "Frame All (A)", set("frame", "\"all\""));
    bar_->addAction(icon("frameSel"), "Frame Selection (F)", set("frame", "\"selection\""));
    bar_->addSeparator();
    bar_->addAction(icon("wireframe"), "Wireframe (4)", set("wireframe", "true"));
    bar_->addAction(icon("shaded"), "Smooth Shade All (5)", set("wireframe", "false"));
    QAction* wos2 = bar_->addAction(icon("wireOnShaded"), "Wireframe on Shaded");
    wos2->setCheckable(true);
    connect(wos2, &QAction::toggled, wos, &QAction::setChecked);
    connect(wos, &QAction::toggled, wos2, &QAction::setChecked);
    QAction* grid2 = bar_->addAction(icon("grid"), "Grid");
    grid2->setCheckable(true);
    grid2->setChecked(true);
    connect(grid2, &QAction::toggled, grid, &QAction::setChecked);
    connect(grid, &QAction::toggled, grid2, &QAction::setChecked);
}

// ---------------------------------------------------------------------------------------------------------------------

PanelLayout::PanelLayout(QWidget* parent) : QWidget(parent) {
    grid_ = new QGridLayout(this);
    grid_->setContentsMargins(0, 0, 0, 0);
    grid_->setSpacing(3);
    // Maya's four views: top, persp / front, side. Single view starts on persp.
    for (const char* cam : {"top", "persp", "front", "side"}) {
        auto* p = new ModelPanel(cam, this);
        connect(p->view(), &ViewportWidget::rightClick, this, &PanelLayout::rightClick);
        connect(p->view(), &ViewportWidget::activated, this, [this](ViewportWidget* v) { last_ = v; });
        panels_.append(p);
    }
    focus_ = 1;
    relayout();
}

void PanelLayout::relayout() {
    for (auto* p : panels_) {
        grid_->removeWidget(p);
        p->hide();
    }
    if (single_) {
        grid_->addWidget(panels_[focus_], 0, 0);
        panels_[focus_]->show();
    } else {
        for (int i = 0; i < 4; ++i) {
            grid_->addWidget(panels_[i], i / 2, i % 2);
            panels_[i]->show();
        }
    }
}

void PanelLayout::set_single(bool single) {
    single_ = single;
    relayout();
}

void PanelLayout::toggle() {
    if (single_) {
        single_ = false;
    } else {
        ViewportWidget* a = active();
        for (int i = 0; i < panels_.size(); ++i)
            if (panels_[i]->view() == a) focus_ = i;
        single_ = true;
    }
    relayout();
}

ViewportWidget* PanelLayout::active() const {
    QPoint g = QCursor::pos();
    for (auto* p : panels_) {
        ViewportWidget* v = p->view();
        if (v->isVisible() && v->rect().contains(v->mapFromGlobal(g))) {
            last_ = v;
            return v;
        }
    }
    if (last_ && last_->isVisible()) return last_;
    return panels_[focus_]->view();
}

QList<ViewportWidget*> PanelLayout::views() const {
    QList<ViewportWidget*> out;
    for (auto* p : panels_) out.append(p->view());
    return out;
}

void PanelLayout::redraw_all() {
    for (auto* p : panels_) p->view()->update();
}

void PanelLayout::set_all(const QString& key, const QString& json) {
    for (auto* p : panels_)
        if (p->isVisible()) p->view()->set(key, json);
}

} // namespace az
