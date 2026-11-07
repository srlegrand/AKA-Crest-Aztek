// The Outliner: the DAG with an icon per type, Display and Show menus, search, drag to parent, middle-drag to reorder.
#include "panels.h"
#include <functional>
#include "host.h"
#include "icons.h"

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QDrag>
#include <QDropEvent>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QLineEdit>
#include <QMenu>
#include <QMimeData>
#include <QMouseEvent>
#include <QScrollBar>
#include <QToolButton>
#include <QTreeWidget>
#include <QVBoxLayout>

namespace az {

namespace {

QJsonObject args(const QJsonArray& a) {
    QJsonObject o;
    o["args"] = a;
    return o;
}

// Dropping on an item parents to it, on the space between items reorders under the same parent, on empty space
// parents to the world. Middle-drag does the same as left-drag, as Maya's.
class OutlinerTree : public QTreeWidget {
public:
    using QTreeWidget::QTreeWidget;
protected:
    void mousePressEvent(QMouseEvent* e) override {
        if (e->button() == Qt::MiddleButton) {
            if (QTreeWidgetItem* it = itemAt(e->position().toPoint())) {
                if (!it->isSelected()) {
                    clearSelection();
                    it->setSelected(true);
                }
                auto* drag = new QDrag(this);
                auto* m = new QMimeData;
                m->setText(it->data(0, Qt::UserRole).toString());
                drag->setMimeData(m);
                drag->exec(Qt::MoveAction);
            }
            return;
        }
        QTreeWidget::mousePressEvent(e);
    }
    void dragEnterEvent(QDragEnterEvent* e) override { e->acceptProposedAction(); }
    void dragMoveEvent(QDragMoveEvent* e) override {
        QTreeWidget::dragMoveEvent(e);
        e->acceptProposedAction();
    }
    void dropEvent(QDropEvent* e) override {
        QPoint at = e->position().toPoint();
        QTreeWidgetItem* target = itemAt(at);
        QJsonArray names;
        for (auto* it : selectedItems()) names.append(it->data(0, Qt::UserRole).toString());
        e->setDropAction(Qt::IgnoreAction);
        e->accept();
        if (names.isEmpty()) return;
        // Near a row's edge: between rows, a reorder among the target's siblings.
        bool between = false;
        if (target) {
            QRect r = visualItemRect(target);
            between = at.y() - r.top() < 4 || r.bottom() - at.y() < 4;
        }
        if (between) {
            QTreeWidgetItem* parent = target->parent();
            QString want = parent ? parent->data(0, Qt::UserRole).toString() : QString();
            QStringList selected_paths;
            for (const QJsonValue& n : names) selected_paths << n.toString();
            // Under the target's parent first, if they aren't yet.
            auto parent_of = [](const QString& p) { return p.left(qMax(0, int(p.lastIndexOf('|')))); };
            bool moved = false;
            for (const QString& p : selected_paths)
                if (parent_of(p) != want) moved = true;
            if (moved) {
                if (want.isEmpty()) {
                    QJsonObject c = args(names);
                    c["world"] = true;
                    exec("parent", c);
                } else {
                    QJsonArray a = names;
                    a.append(want);
                    exec("parent", args(a));
                }
            }
            if (has_reorder()) {
                int to = parent ? parent->indexOfChild(target) : indexOfTopLevelItem(target);
                for (const QJsonValue& n : names) {
                    QString leaf = n.toString().section('|', -1);
                    exec("reorder", QJsonObject{{"args", QJsonArray{want.isEmpty() ? "|" + leaf : want + "|" + leaf}}, {"front", false}, {"relative", to}});
                }
            }
            return;
        }
        if (target) {
            names.append(target->data(0, Qt::UserRole).toString());
            exec("parent", args(names));
        } else {
            QJsonObject call = args(names);
            call["world"] = true;
            exec("parent", call);
        }
    }
private:
    static bool has_reorder() {
        for (const QJsonValue& c : query("commands").toArray())
            if (c.toObject()["name"].toString() == "reorder") return true;
        return false;
    }
};

const char* kTypes[][2] = {{"Joints", "joint"}, {"Polygons", "mesh"}, {"Curves", "nurbsCurve"}, {"Locators", "locator"}, {"Cameras", "camera"}, {"Groups", "group"}, {"IK Handles", "ik"}, {"Deformers", "deformHandle"}, {"Sets", "set"}};

} // namespace

Outliner::Outliner(QWidget* parent) : Panel(parent) {
    auto* lay = new QVBoxLayout(this);
    lay->setContentsMargins(2, 2, 2, 2);
    lay->setSpacing(2);
    auto* top = new QHBoxLayout();
    auto toggle = [this](QMenu* m, const QString& label, bool& flag) {
        QAction* a = m->addAction(label);
        a->setCheckable(true);
        a->setChecked(flag);
        bool* p = &flag;
        connect(a, &QAction::toggled, this, [this, p](bool on) {
            *p = on;
            refresh();
        });
        return a;
    };
    auto* display = new QToolButton(this);
    display->setText("Display");
    display->setPopupMode(QToolButton::InstantPopup);
    auto* menu = new QMenu(display);
    toggle(menu, "DAG Objects Only", dag_only_);
    toggle(menu, "Shapes", shapes_);
    toggle(menu, "Intermediate Objects", intermediates_);
    toggle(menu, "Attributes (Channels)", attrs_);
    toggle(menu, "Hide Default Cameras", hide_cameras_);
    toggle(menu, "Show Icons", show_icons_);
    display->setMenu(menu);
    top->addWidget(display);
    auto* show = new QToolButton(this);
    show->setText("Show");
    show->setPopupMode(QToolButton::InstantPopup);
    auto* show_menu = new QMenu(show);
    QMenu* by_type = show_menu->addMenu("Objects");
    show_menu->addAction("Show All", [this] {
        hidden_types_.clear();
        refresh();
    });
    for (auto& t : kTypes) {
        QString id = t[1];
        QAction* a = by_type->addAction(t[0]);
        a->setCheckable(true);
        a->setChecked(true);
        connect(a, &QAction::toggled, this, [this, id](bool on) {
            if (on) hidden_types_.remove(id);
            else hidden_types_.insert(id);
            refresh();
        });
    }
    show->setMenu(show_menu);
    top->addWidget(show);
    filter_ = new QLineEdit(this);
    filter_->setPlaceholderText("Search...");
    filter_->setClearButtonEnabled(true);
    connect(filter_, &QLineEdit::textChanged, this, [this] { refresh(); });
    top->addWidget(filter_, 1);
    lay->addLayout(top);

    tree_ = new OutlinerTree(this);
    tree_->setHeaderHidden(true);
    tree_->setSelectionMode(QAbstractItemView::ExtendedSelection);
    tree_->setDragDropMode(QAbstractItemView::InternalMove);
    tree_->setEditTriggers(QAbstractItemView::DoubleClicked | QAbstractItemView::EditKeyPressed);
    tree_->setIconSize(QSize(16, 16));
    tree_->setIndentation(14);
    tree_->setUniformRowHeights(true);
    tree_->setObjectName("outliner");
    lay->addWidget(tree_, 1);
    connect(tree_, &QTreeWidget::itemSelectionChanged, this, &Outliner::selection_changed);
    connect(tree_, &QTreeWidget::itemExpanded, this, [this](QTreeWidgetItem* it) { expanded_.insert(it->data(0, Qt::UserRole).toString()); });
    connect(tree_, &QTreeWidget::itemCollapsed, this, [this](QTreeWidgetItem* it) { expanded_.remove(it->data(0, Qt::UserRole).toString()); });
    connect(tree_, &QTreeWidget::itemChanged, this, [this](QTreeWidgetItem* it, int) {
        if (updating_) return;
        QString path = it->data(0, Qt::UserRole).toString();
        QString name = it->text(0).trimmed();
        if (!name.isEmpty() && !path.endsWith("|" + name) && path != name) exec("rename", args(QJsonArray{path, name}));
    });
    tree_->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(tree_, &QWidget::customContextMenuRequested, this, [this](const QPoint& p) {
        QMenu m;
        m.addAction("Select Hierarchy", [] { exec("selectHierarchy"); });
        m.addAction("Rename", [this, p] {
            if (auto* it = tree_->itemAt(p)) tree_->editItem(it);
        });
        m.addAction("Frame Selection", [] { mel("FrameSelected;"); });
        m.addSeparator();
        m.addAction("Expand All", [this] { tree_->expandAll(); });
        m.addAction("Collapse All", [this] { tree_->collapseAll(); });
        m.addSeparator();
        m.addAction("Group", [] { exec("group"); });
        m.addAction("Ungroup", [] { exec("ungroup"); });
        m.addAction("Unparent", [] { exec("parent", QJsonObject{{"world", true}}); });
        m.addSeparator();
        m.addAction("Hide", [] { exec("hide"); });
        m.addAction("Show", [] { exec("show"); });
        m.addAction("Attribute Editor", [] { mel("AttributeEditor;"); });
        m.addSeparator();
        m.addAction("Delete", [] { exec("delete"); });
        m.exec(tree_->viewport()->mapToGlobal(p));
    });
}

void Outliner::add(QTreeWidgetItem* parent, const QJsonObject& node) {
    QString filter = filter_->text().trimmed();
    QString ic = node["icon"].toString();
    bool cam_default = ic == "camera" && QStringList{"persp", "top", "front", "side"}.contains(node["name"].toString());
    if (hide_cameras_ && cam_default) return;
    if (hidden_types_.contains(ic == "objectSet" ? QString("set") : ic)) return;
    if (dag_only_ && (ic == "set" || ic == "objectSet")) return;
    auto* it = parent ? new QTreeWidgetItem(parent) : new QTreeWidgetItem(tree_);
    QString path = node["path"].toString();
    it->setText(0, node["name"].toString());
    it->setData(0, Qt::UserRole, path);
    if (show_icons_) it->setIcon(0, icon(ic, 16));
    it->setFlags(it->flags() | Qt::ItemIsEditable | Qt::ItemIsDragEnabled | Qt::ItemIsDropEnabled);
    // Default nodes dim, as Maya's are.
    if (cam_default) it->setForeground(0, QColor(150, 150, 150));
    if (!node["visible"].toBool()) it->setForeground(0, QColor(130, 130, 130));
    if (node["intermediate"].toBool()) it->setForeground(0, QColor(110, 110, 160));
    for (const QJsonValue& c : node["children"].toArray()) add(it, c.toObject());
    if (attrs_) {
        // Channels as children, Display > Attributes.
        QJsonArray names = ask("listAttr", QJsonObject{{"args", QJsonArray{path}}, {"keyable", true}}).toArray();
        for (const QJsonValue& a : names) {
            auto* ch = new QTreeWidgetItem(it);
            ch->setText(0, a.toString());
            ch->setForeground(0, QColor(170, 190, 210));
            ch->setFlags(Qt::ItemIsEnabled);
            ch->setData(0, Qt::UserRole, path);
        }
    }
    if (node["selected"].toBool()) it->setSelected(true);
    if (expanded_.contains(path)) it->setExpanded(true);
    if (!filter.isEmpty()) {
        // Glob search: *arm*, or plain text anywhere in the name.
        QRegularExpression re(QRegularExpression::wildcardToRegularExpression(filter.contains('*') || filter.contains('?') ? filter : "*" + filter + "*"), QRegularExpression::CaseInsensitiveOption);
        bool match = re.match(node["name"].toString()).hasMatch();
        bool child_visible = false;
        for (int i = 0; i < it->childCount(); ++i) child_visible |= !it->child(i)->isHidden();
        it->setHidden(!match && !child_visible);
        if (child_visible) it->setExpanded(true);
    }
}

void Outliner::set_editor_options(const QJsonObject& p) {
    if (p.contains("showShapes")) shapes_ = p["showShapes"].toBool();
    if (p.contains("showDagOnly")) dag_only_ = p["showDagOnly"].toBool();
    if (p.contains("showAttributes")) attrs_ = p["showAttributes"].toBool();
    if (p.contains("mainListConnection")) list_connection_ = p["mainListConnection"].toString();
    if (p.contains("setFilter")) item_filter_ = p["setFilter"].toString();
    if (p.contains("filter")) item_filter_ = p["filter"].toString();
    schedule();
}

// Only what the editor's connection or filter names, with the items above them.
void Outliner::apply_list_limit() {
    QSet<QString> keep;
    bool limited = false;
    if (!list_connection_.isEmpty()) {
        QString kind = ask("selectionConnection", QJsonObject{{"args", QJsonArray{list_connection_}}, {"query", true}, {"queried", QJsonArray{"identify"}}}).toString();
        if (kind == "objectList") {
            limited = true;
            for (const QJsonValue& v : ask("selectionConnection", QJsonObject{{"args", QJsonArray{list_connection_}}, {"query", true}, {"queried", QJsonArray{"object"}}}).toArray()) keep.insert(v.toString().section('|', -1));
        }
    }
    if (!item_filter_.isEmpty()) {
        QSet<QString> passed;
        for (const QJsonValue& v : ask("lsThroughFilter", QJsonObject{{"args", QJsonArray{item_filter_}}}).toArray()) passed.insert(v.toString().section('|', -1));
        keep = limited ? keep.intersect(passed) : passed;
        limited = true;
    }
    if (!limited) return;
    std::function<bool(QTreeWidgetItem*)> walk = [&](QTreeWidgetItem* it) {
        bool any = keep.contains(it->data(0, Qt::UserRole).toString().section('|', -1));
        for (int i = 0; i < it->childCount(); ++i) any |= walk(it->child(i));
        it->setHidden(!any);
        return any;
    };
    for (int i = 0; i < tree_->topLevelItemCount(); ++i) walk(tree_->topLevelItem(i));
}

void Outliner::refresh() {
    updating_ = true;
    int scroll = tree_->verticalScrollBar()->value();
    tree_->clear();
    for (const QJsonValue& n : query("outliner", intermediates_ ? "shapes intermediate" : shapes_ ? "shapes" : "").toArray()) add(nullptr, n.toObject());
    apply_list_limit();
    tree_->verticalScrollBar()->setValue(scroll);
    updating_ = false;
}

void Outliner::selection_changed() {
    if (updating_) return;
    QJsonArray names;
    for (auto* it : tree_->selectedItems()) names.append(it->data(0, Qt::UserRole).toString());
    if (names.isEmpty())
        exec("select", QJsonObject{{"clear", true}});
    else
        exec("select", args(names));
}

} // namespace az
