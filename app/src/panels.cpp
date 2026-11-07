#include "panels.h"
#include <QStyle>
#include <QMouseEvent>
#include "host.h"
#include "icons.h"

#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QDropEvent>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QScrollArea>
#include <QScrollBar>
#include <QSplitter>
#include <QTabWidget>
#include <QTimer>
#include <QToolBar>
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

QString fmt(double v) {
    // Maya shows three decimals in the channel box.
    QString s = QString::number(v, 'f', 3);
    if (s == "-0.000") s = "0.000";
    return s;
}

QString value_text(const QJsonObject& ch) {
    QJsonValue v = ch["value"];
    QString t = ch["type"].toString();
    if (t == "bool") return v.toBool() ? "on" : "off";
    if (t == "enum") {
        QJsonArray names = ch["enum"].toArray();
        int i = v.toInt();
        if (i >= 0 && i < names.size()) return names[i].toString();
        return QString::number(i);
    }
    if (t == "int") return QString::number(v.toInteger());
    if (t == "float") return fmt(v.toDouble());
    if (t == "string") return v.toString();
    return QString();
}

// Text typed into a field to the setAttr value for its type.
QJsonValue typed(const QString& t, const QString& text, const QJsonObject& ch) {
    QString s = text.trimmed();
    if (t == "bool") return QJsonValue(s == "on" || s == "1" || s == "true" || s == "yes");
    if (t == "int") return QJsonValue(qint64(s.toDouble()));
    if (t == "enum") {
        QJsonArray names = ch["enum"].toArray();
        for (int i = 0; i < names.size(); ++i)
            if (names[i].toString().compare(s, Qt::CaseInsensitive) == 0) return QJsonValue(i);
        return QJsonValue(qint64(s.toDouble()));
    }
    if (t == "float") return QJsonValue(s.toDouble());
    return QJsonValue(s);
}

void set_attr(const QString& plug, const QJsonValue& v, const QString& type) {
    QJsonObject call = args(QJsonArray{plug, v});
    if (type == "string") call["type"] = "string";
    exec("setAttr", call);
}

// Drops reparent: dragging items onto another runs parent; onto empty space parents to the world.
class OutlinerTree : public QTreeWidget {
public:
    using QTreeWidget::QTreeWidget;
protected:
    void dropEvent(QDropEvent* e) override {
        QTreeWidgetItem* target = itemAt(e->position().toPoint());
        QJsonArray names;
        for (auto* it : selectedItems()) names.append(it->data(0, Qt::UserRole).toString());
        if (names.isEmpty()) return;
        e->setDropAction(Qt::IgnoreAction);
        e->accept();
        if (target) {
            QString p = target->data(0, Qt::UserRole).toString();
            names.append(p);
            exec("parent", args(names));
        } else {
            QJsonObject call = args(names);
            call["world"] = true;
            exec("parent", call);
        }
    }
};

} // namespace

// ---------------------------------------------------------------------------------------------------------------------

Panel::Panel(QWidget* parent) : QWidget(parent) {
    connect(Host::get(), &Host::sceneChanged, this, &Panel::schedule);
}

void Panel::schedule() {
    if (pending_) return;
    pending_ = true;
    QTimer::singleShot(0, this, [this] {
        pending_ = false;
        if (!isVisible()) return;
        // Don't pull a field out from under the user's typing.
        QWidget* f = QApplication::focusWidget();
        if (f && isAncestorOf(f) && f->property("editing").toBool()) return;
        refresh();
    });
}

void Panel::showEvent(QShowEvent*) {
    schedule();
}

// ---------------------------------------------------------------------------------------------------------------------

Outliner::Outliner(QWidget* parent) : Panel(parent) {
    auto* lay = new QVBoxLayout(this);
    lay->setContentsMargins(2, 2, 2, 2);
    lay->setSpacing(2);
    auto* top = new QHBoxLayout();
    auto* display = new QToolButton(this);
    display->setText("Display");
    display->setPopupMode(QToolButton::InstantPopup);
    auto* menu = new QMenu(display);
    QAction* shapes = menu->addAction("Shapes");
    shapes->setCheckable(true);
    connect(shapes, &QAction::toggled, this, [this](bool on) {
        shapes_ = on;
        refresh();
    });
    display->setMenu(menu);
    top->addWidget(display);
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
        m.addAction("Select Hierarchy", [] { exec("select", QJsonObject{{"hierarchy", true}}); });
        m.addAction("Rename", [this, p] {
            if (auto* it = tree_->itemAt(p)) tree_->editItem(it);
        });
        m.addSeparator();
        m.addAction("Group", [] { exec("group"); });
        m.addAction("Unparent", [] { exec("parent", QJsonObject{{"world", true}}); });
        m.addAction("Delete", [] { exec("delete"); });
        m.exec(tree_->viewport()->mapToGlobal(p));
    });
}

void Outliner::add(QTreeWidgetItem* parent, const QJsonObject& node) {
    QString filter = filter_->text().trimmed();
    auto* it = parent ? new QTreeWidgetItem(parent) : new QTreeWidgetItem(tree_);
    QString path = node["path"].toString();
    it->setText(0, node["name"].toString());
    it->setData(0, Qt::UserRole, path);
    it->setIcon(0, icon(node["icon"].toString(), 16));
    it->setFlags(it->flags() | Qt::ItemIsEditable | Qt::ItemIsDragEnabled | Qt::ItemIsDropEnabled);
    if (!node["visible"].toBool()) it->setForeground(0, QColor(130, 130, 130));
    if (node["intermediate"].toBool()) it->setForeground(0, QColor(110, 110, 160));
    for (const QJsonValue& c : node["children"].toArray()) add(it, c.toObject());
    if (node["selected"].toBool()) it->setSelected(true);
    if (expanded_.contains(path)) it->setExpanded(true);
    if (!filter.isEmpty()) {
        bool match = node["name"].toString().contains(filter, Qt::CaseInsensitive);
        bool child_visible = false;
        for (int i = 0; i < it->childCount(); ++i) child_visible |= !it->child(i)->isHidden();
        it->setHidden(!match && !child_visible);
        if (child_visible) it->setExpanded(true);
    }
}

void Outliner::refresh() {
    updating_ = true;
    int scroll = tree_->verticalScrollBar()->value();
    tree_->clear();
    for (const QJsonValue& n : query("outliner", shapes_ ? "shapes" : "").toArray()) add(nullptr, n.toObject());
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

// ---------------------------------------------------------------------------------------------------------------------

// Channel names picked in the channel box (Maya's highlighted channels): their attribute names, kept across refreshes.
static QStringList g_channel_sel;
static QList<QLabel*> g_channel_labels;

QStringList selected_channels() {
    return g_channel_sel;
}

void clear_channel_selection() {
    g_channel_sel.clear();
}

namespace {

void restyle_channel_labels() {
    for (QLabel* l : g_channel_labels) {
        l->setProperty("picked", g_channel_sel.contains(l->property("attr").toString()));
        l->style()->unpolish(l);
        l->style()->polish(l);
    }
}

// A channel name: click picks it, Ctrl+click toggles, Shift+click adds.
class ChannelLabel : public QLabel {
public:
    ChannelLabel(const QString& text, QWidget* parent) : QLabel(text, parent) {}
protected:
    void mousePressEvent(QMouseEvent* e) override {
        if (e->button() != Qt::LeftButton) return QLabel::mousePressEvent(e);
        QString a = property("attr").toString();
        if (e->modifiers() & Qt::ControlModifier) {
            if (g_channel_sel.contains(a)) g_channel_sel.removeAll(a);
            else g_channel_sel << a;
        } else if (e->modifiers() & Qt::ShiftModifier) {
            if (!g_channel_sel.contains(a)) g_channel_sel << a;
        } else {
            g_channel_sel = {a};
        }
        restyle_channel_labels();
    }
};

// A channel row: right-aligned name, value field. Enter commits a setAttr.
QWidget* channel_row(const QJsonObject& ch, QWidget* parent, int label_width) {
    auto* row = new QWidget(parent);
    auto* h = new QHBoxLayout(row);
    h->setContentsMargins(0, 0, 0, 0);
    h->setSpacing(4);
    auto* label = new ChannelLabel(ch["nice"].toString(), row);
    label->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    label->setFixedWidth(label_width);
    label->setToolTip(ch["plug"].toString());
    label->setObjectName("channelName");
    label->setProperty("attr", ch["attr"].toString());
    label->setProperty("picked", g_channel_sel.contains(ch["attr"].toString()));
    g_channel_labels << label;
    auto* edit = new QLineEdit(value_text(ch), row);
    // Maya's channel colours: keyed, key on this frame, changed since keyed, driven, locked.
    QString st = ch["state"].toString();
    edit->setObjectName(st == "keyed" ? "channelKeyed" : st == "keyOnFrame" ? "channelKeyOnFrame" : st == "changed" ? "channelChanged"
                        : st == "muted" ? "channelMuted" : st == "locked" ? "channelLocked" : (st == "connected" || ch["connected"].toBool()) ? "channelConnected" : "channelValue");
    edit->setFixedHeight(18);
    // Right-click is the channel box's menu, not the text field's.
    edit->setContextMenuPolicy(Qt::NoContextMenu);
    QString plug = ch["plug"].toString(), type = ch["type"].toString();
    QJsonObject c = ch;
    QObject::connect(edit, &QLineEdit::textEdited, edit, [edit] { edit->setProperty("editing", true); });
    QObject::connect(edit, &QLineEdit::editingFinished, edit, [edit, plug, type, c] {
        if (!edit->property("editing").toBool()) return;
        edit->setProperty("editing", false);
        QString text = edit->text();
        // Maya's channel box takes relative edits: +=1, -=2, *=2, /=2.
        double current = c["value"].toDouble();
        if (type == "float" || type == "int") {
            QString t = text.trimmed();
            for (QString op : {"+=", "-=", "*=", "/="})
                if (t.startsWith(op)) {
                    double x = t.mid(2).toDouble();
                    double r = op == "+=" ? current + x : op == "-=" ? current - x : op == "*=" ? current * x : (x != 0 ? current / x : current);
                    text = QString::number(r, 'g', 12);
                }
        }
        set_attr(plug, typed(type, text, c), type);
    });
    h->addWidget(label);
    h->addWidget(edit, 1);
    return row;
}

QWidget* section(const QString& text, QWidget* parent) {
    auto* l = new QLabel(text, parent);
    l->setObjectName("channelSection");
    return l;
}

} // namespace

ChannelBox::ChannelBox(QWidget* parent) : Panel(parent) {
    auto* lay = new QVBoxLayout(this);
    lay->setContentsMargins(0, 0, 0, 0);
    scroll_ = new QScrollArea(this);
    scroll_->setWidgetResizable(true);
    scroll_->setFrameShape(QFrame::NoFrame);
    lay->addWidget(scroll_);
    setObjectName("channelBox");
    // Maya's channel box menu: keys, locks and connections on the picked channels.
    setContextMenuPolicy(Qt::CustomContextMenu);
    connect(this, &QWidget::customContextMenuRequested, this, [this](const QPoint& p) {
        QMenu m;
        QJsonArray picked;
        for (const QString& a : g_channel_sel) picked.append(a);
        bool any = !picked.isEmpty();
        auto plugs = [] {
            QJsonArray out;
            QJsonArray sel = state()["selection"].toArray();
            for (const QJsonValue& o : sel)
                for (const QString& a : g_channel_sel) out.append(o.toString().section('.', 0, 0) + "." + a);
            return out;
        };
        m.addAction("Key Selected", [picked] { exec("setKeyframe", QJsonObject{{"attribute", picked}}); })->setEnabled(any);
        m.addAction("Key All", [] { exec("setKeyframe"); });
        m.addAction("Breakdown Selected", [picked] { exec("setKeyframe", QJsonObject{{"attribute", picked}, {"breakdown", true}}); })->setEnabled(any);
        m.addAction("Breakdown All", [] { exec("setKeyframe", QJsonObject{{"breakdown", true}}); });
        m.addSeparator();
        m.addAction("Mute Selected", [plugs] {
            for (const QJsonValue& pl : plugs()) exec("mute", QJsonObject{{"args", QJsonArray{pl}}});
        })->setEnabled(any);
        m.addAction("Mute All", [] { exec("mute"); });
        m.addAction("Unmute Selected", [plugs] {
            for (const QJsonValue& pl : plugs()) exec("mute", QJsonObject{{"args", QJsonArray{pl}}, {"disable", true}, {"force", true}});
        })->setEnabled(any);
        m.addAction("Unmute All", [] { exec("mute", QJsonObject{{"disable", true}, {"force", true}}); });
        m.addSeparator();
        m.addAction("Delete Selected Keys", [picked] { exec("cutKey", QJsonObject{{"attribute", picked}}); })->setEnabled(any);
        m.addAction("Break Connections", [plugs] {
            for (const QJsonValue& pl : plugs()) exec("disconnectAttr", QJsonObject{{"args", QJsonArray{pl}}});
        })->setEnabled(any);
        m.addSeparator();
        m.addAction("Lock Selected", [plugs] {
            for (const QJsonValue& pl : plugs()) exec("setAttr", QJsonObject{{"args", QJsonArray{pl}}, {"lock", true}});
        })->setEnabled(any);
        m.addAction("Unlock Selected", [plugs] {
            for (const QJsonValue& pl : plugs()) exec("setAttr", QJsonObject{{"args", QJsonArray{pl}}, {"lock", false}});
        })->setEnabled(any);
        m.addAction("Hide Selected", [plugs] {
            for (const QJsonValue& pl : plugs()) exec("setAttr", QJsonObject{{"args", QJsonArray{pl}}, {"keyable", false}});
        })->setEnabled(any);
        m.addAction("Lock and Hide Selected", [plugs] {
            for (const QJsonValue& pl : plugs()) exec("setAttr", QJsonObject{{"args", QJsonArray{pl}}, {"lock", true}, {"keyable", false}});
        })->setEnabled(any);
        m.addSeparator();
        m.addAction("Select All Channels", [] {
            g_channel_sel.clear();
            for (QLabel* l : g_channel_labels) g_channel_sel << l->property("attr").toString();
            restyle_channel_labels();
        });
        m.addAction("Deselect Channels", [] {
            g_channel_sel.clear();
            restyle_channel_labels();
        });
        m.exec(mapToGlobal(p));
    });
}

void ChannelBox::add_channels(QVBoxLayout* lay, const QJsonArray& channels) {
    QFontMetrics fm(font());
    int w = 60;
    for (const QJsonValue& c : channels) w = qMax(w, fm.horizontalAdvance(c.toObject()["nice"].toString()) + 8);
    for (const QJsonValue& c : channels) lay->addWidget(channel_row(c.toObject(), body_, w));
}

void ChannelBox::refresh() {
    QJsonObject cb = query("channelBox").toObject();
    g_channel_labels.clear();
    int scroll = scroll_->verticalScrollBar()->value();
    body_ = new QWidget();
    auto* lay = new QVBoxLayout(body_);
    lay->setContentsMargins(4, 4, 4, 4);
    lay->setSpacing(1);
    if (cb.isEmpty()) {
        lay->addStretch(1);
        scroll_->setWidget(body_);
        return;
    }
    // Object name, editable to rename.
    auto* name = new QLineEdit(cb["name"].toString(), body_);
    name->setObjectName("channelHeader");
    QString path = cb["path"].toString();
    connect(name, &QLineEdit::textEdited, name, [name] { name->setProperty("editing", true); });
    connect(name, &QLineEdit::editingFinished, name, [name, path] {
        if (!name->property("editing").toBool()) return;
        name->setProperty("editing", false);
        exec("rename", args(QJsonArray{path, name->text().trimmed()}));
    });
    lay->addWidget(name);
    add_channels(lay, cb["channels"].toArray());
    QJsonArray shapes = cb["shapes"].toArray();
    if (!shapes.isEmpty()) {
        lay->addSpacing(6);
        lay->addWidget(section("SHAPES", body_));
        for (const QJsonValue& s : shapes) {
            auto* l = new QLabel(s.toObject()["name"].toString(), body_);
            l->setObjectName("channelNode");
            lay->addWidget(l);
            add_channels(lay, s.toObject()["channels"].toArray());
        }
    }
    QJsonArray inputs = cb["inputs"].toArray();
    if (!inputs.isEmpty()) {
        lay->addSpacing(6);
        lay->addWidget(section("INPUTS", body_));
        for (const QJsonValue& v : inputs) {
            QJsonObject in = v.toObject();
            QString p = in["path"].toString();
            auto* b = new QPushButton(in["name"].toString(), body_);
            b->setObjectName("channelNodeButton");
            b->setFlat(true);
            connect(b, &QPushButton::clicked, this, [this, p] {
                if (open_inputs_.contains(p))
                    open_inputs_.remove(p);
                else
                    open_inputs_.insert(p);
                refresh();
            });
            lay->addWidget(b);
            if (open_inputs_.contains(p)) add_channels(lay, in["channels"].toArray());
        }
    }
    lay->addStretch(1);
    scroll_->setWidget(body_);
    scroll_->verticalScrollBar()->setValue(scroll);
}

// ---------------------------------------------------------------------------------------------------------------------

LayerEditor::LayerEditor(QWidget* parent) : Panel(parent) {
    auto* lay = new QVBoxLayout(this);
    lay->setContentsMargins(2, 2, 2, 2);
    lay->setSpacing(2);
    auto* bar = new QToolBar(this);
    bar->setIconSize(QSize(16, 16));
    bar->addAction(icon("layer"), "Create a new layer", [] { exec("createDisplayLayer", QJsonObject{{"empty", true}}); });
    bar->addAction(icon("layerSelected"), "Create a new layer and assign selected objects", [] { exec("createDisplayLayer"); });
    lay->addWidget(bar);
    list_ = new QTreeWidget(this);
    list_->setColumnCount(3);
    list_->setHeaderHidden(true);
    list_->setRootIsDecorated(false);
    list_->header()->setStretchLastSection(true);
    list_->setColumnWidth(0, 26);
    list_->setColumnWidth(1, 26);
    lay->addWidget(list_, 1);
    connect(list_, &QTreeWidget::itemClicked, this, [](QTreeWidgetItem* it, int col) {
        QString layer = it->data(0, Qt::UserRole).toString();
        if (col == 0) {
            bool vis = it->data(0, Qt::UserRole + 1).toBool();
            exec("setAttr", args(QJsonArray{layer + ".visibility", !vis}));
        } else if (col == 1) {
            // Cycles normal, template, reference as Maya's display type button.
            int t = (it->data(1, Qt::UserRole).toInt() + 1) % 3;
            exec("setAttr", args(QJsonArray{layer + ".displayType", t}));
        }
    });
    connect(list_, &QTreeWidget::itemDoubleClicked, this, [](QTreeWidgetItem* it, int col) {
        if (col == 2) exec("select", args(it->data(2, Qt::UserRole).toJsonArray()));
    });
    list_->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(list_, &QWidget::customContextMenuRequested, this, [this](const QPoint& p) {
        QTreeWidgetItem* it = list_->itemAt(p);
        if (!it) return;
        QString layer = it->data(0, Qt::UserRole).toString();
        QJsonArray members = it->data(2, Qt::UserRole).toJsonArray();
        QMenu m;
        m.addAction("Add Selected Objects", [layer] {
            QJsonArray a{layer};
            for (const QJsonValue& s : state()["selection"].toArray()) a.append(s);
            exec("editDisplayLayerMembers", args(a));
        });
        m.addAction("Remove Selected Objects", [] { exec("removeFromDisplayLayer"); });
        m.addAction("Select Objects", [members] { exec("select", args(members)); });
        m.addSeparator();
        m.addAction("Delete Layer", [layer] { exec("deleteDisplayLayer", args(QJsonArray{layer})); });
        m.exec(list_->viewport()->mapToGlobal(p));
    });
}

void LayerEditor::refresh() {
    list_->clear();
    for (const QJsonValue& v : query("layers").toArray()) {
        QJsonObject l = v.toObject();
        auto* it = new QTreeWidgetItem(list_);
        bool vis = l["visible"].toBool();
        int type = l["displayType"].toInt();
        it->setText(0, vis ? "V" : "");
        it->setText(1, type == 1 ? "T" : type == 2 ? "R" : "");
        it->setText(2, l["name"].toString());
        it->setData(0, Qt::UserRole, l["name"].toString());
        it->setData(0, Qt::UserRole + 1, vis);
        it->setData(1, Qt::UserRole, type);
        it->setData(2, Qt::UserRole, l["members"].toArray());
        it->setTextAlignment(0, Qt::AlignCenter);
        it->setTextAlignment(1, Qt::AlignCenter);
        it->setToolTip(2, QString("%1 members. Double-click to select them.").arg(l["members"].toArray().size()));
    }
}

// ---------------------------------------------------------------------------------------------------------------------

namespace {

QWidget* attr_field(const QJsonObject& a, QWidget* parent) {
    QString type = a["type"].toString(), plug = a["plug"].toString();
    bool ro = a["output"].toBool() || a["connected"].toBool();
    QJsonObject c = a;
    if (type == "bool") {
        auto* cb = new QCheckBox(parent);
        cb->setChecked(a["value"].toBool());
        cb->setEnabled(!ro);
        QObject::connect(cb, &QCheckBox::toggled, cb, [plug](bool on) { set_attr(plug, on, "bool"); });
        return cb;
    }
    if (type == "enum") {
        auto* box = new QComboBox(parent);
        for (const QJsonValue& n : a["enum"].toArray()) box->addItem(n.toString());
        box->setCurrentIndex(a["value"].toInt());
        box->setEnabled(!ro);
        QObject::connect(box, &QComboBox::activated, box, [plug](int i) { set_attr(plug, i, "int"); });
        return box;
    }
    if (type == "vec3") {
        auto* w = new QWidget(parent);
        auto* h = new QHBoxLayout(w);
        h->setContentsMargins(0, 0, 0, 0);
        h->setSpacing(3);
        QJsonArray v = a["value"].toArray();
        QJsonArray kids = a["children"].toArray();
        for (int k = 0; k < 3; ++k) {
            auto* e = new QLineEdit(fmt(v.at(k).toDouble()), w);
            e->setReadOnly(ro);
            QString child = kids.at(k).toString();
            if (child.isEmpty()) child = a["attr"].toString() + QString("XYZ")[k];
            QString node = plug.left(plug.lastIndexOf('.'));
            QString cp = node + "." + child;
            QObject::connect(e, &QLineEdit::textEdited, e, [e] { e->setProperty("editing", true); });
            QObject::connect(e, &QLineEdit::editingFinished, e, [e, cp] {
                if (!e->property("editing").toBool()) return;
                e->setProperty("editing", false);
                set_attr(cp, e->text().toDouble(), "float");
            });
            h->addWidget(e);
        }
        return w;
    }
    if (type == "float" || type == "int" || type == "string") {
        auto* e = new QLineEdit(value_text(a), parent);
        e->setReadOnly(ro);
        QObject::connect(e, &QLineEdit::textEdited, e, [e] { e->setProperty("editing", true); });
        QObject::connect(e, &QLineEdit::editingFinished, e, [e, plug, type, c] {
            if (!e->property("editing").toBool()) return;
            e->setProperty("editing", false);
            set_attr(plug, typed(type, e->text(), c), type);
        });
        return e;
    }
    auto* l = new QLabel(type == "matrix" ? "(matrix)" : "", parent);
    return l;
}

} // namespace

AttributeEditor::AttributeEditor(QWidget* parent) : Panel(parent) {
    auto* lay = new QVBoxLayout(this);
    lay->setContentsMargins(2, 2, 2, 2);
    tabs_ = new QTabWidget(this);
    tabs_->setDocumentMode(true);
    tabs_->setUsesScrollButtons(true);
    lay->addWidget(tabs_);
    connect(tabs_, &QTabWidget::currentChanged, this, [this](int i) {
        if (i >= 0) current_tab_ = tabs_->tabText(i);
    });
}

void AttributeEditor::refresh() {
    QJsonObject ae = query("attributeEditor", "").toObject();
    QString keep = current_tab_;
    tabs_->blockSignals(true);
    while (tabs_->count()) {
        QWidget* w = tabs_->widget(0);
        tabs_->removeTab(0);
        w->deleteLater();
    }
    int select = 0;
    for (const QJsonValue& tv : ae["tabs"].toArray()) {
        QJsonObject t = tv.toObject();
        auto* scroll = new QScrollArea();
        scroll->setWidgetResizable(true);
        scroll->setFrameShape(QFrame::NoFrame);
        auto* body = new QWidget();
        auto* v = new QVBoxLayout(body);
        v->setContentsMargins(6, 6, 6, 6);
        auto* head = new QHBoxLayout();
        head->addWidget(new QLabel(QString("<b>%1:</b>").arg(t["type"].toString()), body));
        auto* name = new QLineEdit(t["name"].toString(), body);
        QString path = t["path"].toString();
        connect(name, &QLineEdit::editingFinished, name, [name, path] {
            if (name->text().trimmed() != path.section('|', -1)) exec("rename", args(QJsonArray{path, name->text().trimmed()}));
        });
        head->addWidget(name, 1);
        v->addLayout(head);
        // Grouped by category, in the order they come.
        QMap<QString, QFormLayout*> forms;
        QStringList order;
        for (const QJsonValue& av : t["attrs"].toArray()) {
            QJsonObject a = av.toObject();
            if (a["type"].toString() == "matrix") continue;
            QString cat = a["category"].toString();
            if (cat.isEmpty()) cat = QString("%1 Attributes").arg(t["type"].toString().left(1).toUpper() + t["type"].toString().mid(1));
            if (!forms.contains(cat)) {
                auto* box = new QLabel(cat, body);
                box->setObjectName("aeSection");
                v->addWidget(box);
                auto* f = new QFormLayout();
                f->setLabelAlignment(Qt::AlignRight);
                f->setContentsMargins(8, 2, 2, 6);
                f->setVerticalSpacing(3);
                v->addLayout(f);
                forms[cat] = f;
                order << cat;
            }
            QWidget* field = attr_field(a, body);
            field->setToolTip(a["doc"].toString());
            forms[cat]->addRow(a["nice"].toString(), field);
        }
        v->addStretch(1);
        scroll->setWidget(body);
        int idx = tabs_->addTab(scroll, t["name"].toString());
        if (t["name"].toString() == keep) select = idx;
    }
    tabs_->setCurrentIndex(select);
    tabs_->blockSignals(false);
}

// ---------------------------------------------------------------------------------------------------------------------

ScriptEditor::ScriptEditor(QWidget* parent) : QWidget(parent) {
    auto* lay = new QVBoxLayout(this);
    lay->setContentsMargins(0, 0, 0, 0);
    lay->setSpacing(0);
    auto* bar = new QToolBar(this);
    bar->setIconSize(QSize(18, 18));
    bar->addAction(icon("clearHistory"), "Clear History", [this] { history_->clear(); });
    bar->addAction(icon("clearInput"), "Clear Input", [this] {
        if (auto* e = qobject_cast<QPlainTextEdit*>(inputs_->currentWidget())) e->clear();
    });
    bar->addSeparator();
    bar->addAction(icon("play"), "Execute All (Ctrl+Enter)", [this] { run(false); });
    lay->addWidget(bar);
    auto* split = new QSplitter(Qt::Vertical, this);
    history_ = new QPlainTextEdit(split);
    history_->setReadOnly(true);
    history_->setObjectName("scriptHistory");
    history_->setMaximumBlockCount(20000);
    inputs_ = new QTabWidget(split);
    inputs_->setDocumentMode(true);
    for (const char* lang : {"MEL", "Python"}) {
        // Python tab first opened with the usual import.
        auto* e = new QPlainTextEdit(inputs_);
        e->setObjectName("scriptInput");
        e->setTabStopDistance(4 * e->fontMetrics().horizontalAdvance(' '));
        e->installEventFilter(this);
        if (QString(lang) == "Python") e->setPlainText("import maya.cmds as cmds\n");
        inputs_->addTab(e, lang);
    }
    split->setStretchFactor(0, 3);
    split->setStretchFactor(1, 2);
    lay->addWidget(split, 1);
    QFont mono("monospace");
    mono.setStyleHint(QFont::Monospace);
    mono.setPointSize(9);
    history_->setFont(mono);
    for (int i = 0; i < inputs_->count(); ++i) inputs_->widget(i)->setFont(mono);
}

void ScriptEditor::append(const QJsonArray& lines) {
    static const char* colors[] = {"#d2d2d2", "#9fd0ff", "#ff8080", "#a0a0a0", "#f0c060"};
    for (const QJsonValue& v : lines) {
        QJsonObject l = v.toObject();
        int k = qBound(0, l["kind"].toInt(), 4);
        history_->appendHtml(QString("<span style='color:%1; white-space:pre'>%2</span>").arg(colors[k], l["text"].toString().toHtmlEscaped()));
    }
    history_->verticalScrollBar()->setValue(history_->verticalScrollBar()->maximum());
}

void ScriptEditor::run(bool clear_after) {
    auto* e = qobject_cast<QPlainTextEdit*>(inputs_->currentWidget());
    if (!e) return;
    QString src = e->textCursor().hasSelection() ? e->textCursor().selectedText().replace(QChar(0x2029), '\n') : e->toPlainText();
    if (inputs_->currentIndex() == 1)
        python(src);
    else
        mel(src);
    if (clear_after) e->clear();
}

bool ScriptEditor::eventFilter(QObject* o, QEvent* ev) {
    if (ev->type() == QEvent::KeyPress) {
        auto* k = static_cast<QKeyEvent*>(ev);
        bool enter = k->key() == Qt::Key_Return || k->key() == Qt::Key_Enter;
        // Ctrl+Enter runs and keeps the text; keypad Enter runs and clears it.
        if (enter && (k->modifiers() & Qt::ControlModifier)) {
            run(false);
            return true;
        }
        if (k->key() == Qt::Key_Enter && (k->modifiers() & Qt::KeypadModifier)) {
            run(true);
            return true;
        }
    }
    return QWidget::eventFilter(o, ev);
}

} // namespace az
