// The UV Editor: the selected meshes' UVs in 0-1 space, picked and moved by UV, shell, edge or face, with Maya's
// projection, unfold, layout, cut and sew tools.
#pragma once
#include <QHash>
#include <QJsonArray>
#include <array>
#include <QJsonObject>
#include <QPointF>
#include <QSet>
#include <QVector>
#include <QWidget>

namespace az {

struct UvMesh {
    QString path;
    QVector<QPointF> maps;
    QVector<int> map_point;
    struct Face {
        int id = 0, shell = 0;
        QVector<int> maps, edges;
    };
    QVector<Face> faces;
    // map a, map b, edge
    QVector<std::array<int, 3>> borders;
    QSet<int> sel_faces, sel_edges, sel_points;
};

class UvCanvas : public QWidget {
    Q_OBJECT
public:
    explicit UvCanvas(QWidget* parent = nullptr);
    void set_data(const QJsonObject& d);
    void set_mode(const QString& m);
    QString mode() const { return mode_; }
    void set_tool(const QString& t);
    void frame(bool selected);
    // The UVs to act on, as polyEditUV arguments (pCube1.map[...]); falls back to the scene selection.
    QJsonArray targets() const;
    bool show_checker = false;
    bool shaded = true;
signals:
    void picked();
protected:
    void paintEvent(QPaintEvent*) override;
    void mousePressEvent(QMouseEvent*) override;
    void mouseMoveEvent(QMouseEvent*) override;
    void mouseReleaseEvent(QMouseEvent*) override;
    void wheelEvent(QWheelEvent*) override;
    void keyPressEvent(QKeyEvent*) override;
    void contextMenuEvent(QContextMenuEvent*) override;
private:
    QPointF to_screen(QPointF uv) const;
    QPointF to_uv(QPointF p) const;
    // Selected maps per mesh, from the local UV selection or the scene's components.
    QHash<int, QSet<int>> active_maps() const;
    QPointF selection_center() const;
    void pick(QPointF pos, QPointF to, bool rect, Qt::KeyboardModifiers mods);
    void drag_to(QPointF pos, bool final);
    int handle_at(QPointF pos) const;

    QVector<UvMesh> meshes_;
    QString scene_mode_ = "object";
    QString mode_ = "uv";
    QString tool_ = "move";
    // Local UV selection: mesh index -> maps.
    QHash<QString, QSet<int>> uv_sel_;
    // View: uv at the canvas centre and pixels per uv unit.
    QPointF center_{0.5, 0.5};
    double scale_ = 400;
    bool framed_ = false;
    enum class Mode { None, Pan, Zoom, Marquee, Drag } drag_ = Mode::None;
    QPointF press_, last_;
    QPointF pivot_;
    int handle_ = 0;
    bool applied_ = false;
    QJsonArray drag_targets_;
};

class UvEditor : public QWidget {
    Q_OBJECT
public:
    explicit UvEditor(QWidget* parent = nullptr);
    void refresh();
private:
    void run(const QString& cmd, const QJsonObject& call = QJsonObject(), bool on_selection = true);
    UvCanvas* canvas_ = nullptr;
    QHash<QString, QAction*> mode_buttons_;
};

} // namespace az
