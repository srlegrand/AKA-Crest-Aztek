// The Attribute Editor: a tab per node in the chain, sections by node type, sliders on ranged attributes, a Notes box, and
// Select, Load Attributes and Copy Tab along the bottom.
#include "panels.h"
#include "host.h"
#include "icons.h"

#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QScrollArea>
#include <QSlider>
#include <QTabBar>
#include <QTabWidget>
#include <QToolButton>
#include <QVBoxLayout>

namespace az {

namespace {

QJsonObject args(const QJsonArray& a) {
    QJsonObject o;
    o["args"] = a;
    return o;
}

QString fmt3(double v) {
    QString s = QString::number(v, 'f', 3);
    return s == "-0.000" ? "0.000" : s;
}

void set_plug(const QString& plug, const QJsonValue& v, const QString& type) {
    QJsonObject call = args(QJsonArray{plug, v});
    if (type == "string") call["type"] = "string";
    exec("setAttr", call);
}

// Which section an attribute falls in, by node type, in the order sections come. Anything else goes to Extra Attributes.
struct Section { const char* title; const char* attrs; };

const QList<Section>& sections_for(const QString& type) {
    static const QList<Section> xf{
        {"Transform Attributes", "translate rotate rotateOrder scale shear rotateAxis inheritsTransform"},
        {"Pivots", "rotatePivot scalePivot rotatePivotTranslate scalePivotTranslate displayRotatePivot displayScalePivot"},
        {"Limit Information", "minTransLimit maxTransLimit minRotLimit maxRotLimit minScaleLimit maxScaleLimit"},
        {"Display", "visibility template displayHandle displayLocalAxis intermediateObject overrideEnabled overrideDisplayType overrideColor overrideRGBColors overrideColorRGB"},
        {"Node Behaviour", "caching nodeState frozen"}};
    static const QList<Section> joint{
        {"Transform Attributes", "translate rotate rotateOrder scale shear inheritsTransform"},
        {"Joint", "jointOrient jointType segmentScaleCompensate preferredAngle stiffness radius drawStyle side type otherType"},
        {"Limit Information", "minRotLimit maxRotLimit minTransLimit maxTransLimit"},
        {"Display", "visibility displayLocalAxis overrideEnabled overrideDisplayType overrideColor overrideRGBColors overrideColorRGB"},
        {"Node Behaviour", "caching nodeState frozen"}};
    static const QList<Section> mesh{
        {"Mesh Component Display", "displayVertices displayEdges displayBorders displayNormal normalSize backfaceCulling"},
        {"Smooth Mesh", "displaySmoothMesh smoothLevel smoothPreview"},
        {"Symmetry", "symmetry symmetryPlane"},
        {"Display", "visibility intermediateObject overrideEnabled overrideDisplayType overrideColor overrideRGBColors overrideColorRGB"},
        {"Node Behaviour", "caching nodeState frozen"}};
    static const QList<Section> camera{
        {"Camera Attributes", "focalLength lensSqueezeRatio cameraScale nearClipPlane farClipPlane centerOfInterest"},
        {"Film Back", "horizontalFilmAperture verticalFilmAperture filmFit filmFitOffset horizontalFilmOffset verticalFilmOffset"},
        {"Orthographic Views", "orthographic orthographicWidth"},
        {"Display Options", "displayFilmGate displayResolution displayGateMask overscan"},
        {"Display", "visibility"}};
    static const QList<Section> curve{
        {"NURBS Curve History", "degree form spans"},
        {"Component Display", "dispCV dispEP dispHull lineWidth"},
        {"Display", "visibility intermediateObject overrideEnabled overrideDisplayType overrideColor overrideRGBColors overrideColorRGB"},
        {"Node Behaviour", "caching nodeState frozen"}};
    static const QList<Section> skin{
        {"Skin Cluster Attributes", "envelope skinningMethod useComponents maxInfluences maintainMaxInfluences normalizeWeights weightDistribution deformUserNormals"},
        {"Node Behaviour", "caching nodeState frozen"}};
    static const QList<Section> other{{"Node Behaviour", "caching nodeState frozen"}};
    if (type == "transform" || type == "ikHandle" || type == "locator") return xf;
    if (type == "joint") return joint;
    if (type == "mesh") return mesh;
    if (type == "camera") return camera;
    if (type == "nurbsCurve") return curve;
    if (type == "skinCluster") return skin;
    return other;
}

// A number with a slider beside it, when the attribute has a range.
QWidget* ranged_field(const QJsonObject& a, double lo, double hi, QWidget* parent) {
    QString plug = a["plug"].toString(), type = a["type"].toString();
    auto* row = new QWidget(parent);
    auto* h = new QHBoxLayout(row);
    h->setContentsMargins(0, 0, 0, 0);
    h->setSpacing(4);
    auto* e = new QLineEdit(type == "int" ? QString::number(a["value"].toInteger()) : fmt3(a["value"].toDouble()), row);
    e->setFixedWidth(64);
    auto* s = new QSlider(Qt::Horizontal, row);
    s->setRange(0, 1000);
    auto to_pos = [lo, hi](double v) { return int(qBound(0.0, (v - lo) / qMax(1e-9, hi - lo), 1.0) * 1000); };
    s->setValue(to_pos(a["value"].toDouble()));
    bool ro = a["output"].toBool() || a["connected"].toBool();
    e->setReadOnly(ro);
    s->setEnabled(!ro);
    bool is_int = type == "int";
    auto value_of = [lo, hi, is_int](int p) {
        double v = lo + (hi - lo) * p / 1000.0;
        return is_int ? QJsonValue(qint64(std::round(v))) : QJsonValue(v);
    };
    QObject::connect(s, &QSlider::sliderMoved, e, [e, value_of, plug, is_int](int p) {
        QJsonValue v = value_of(p);
        e->setText(is_int ? QString::number(v.toInteger()) : fmt3(v.toDouble()));
        exec_quiet("setAttr", args(QJsonArray{plug, v}));
        Host::get()->poke();
    });
    QObject::connect(s, &QSlider::sliderReleased, e, [s, value_of, plug] { exec("setAttr", args(QJsonArray{plug, value_of(s->value())})); });
    QObject::connect(e, &QLineEdit::textEdited, e, [e] { e->setProperty("editing", true); });
    QObject::connect(e, &QLineEdit::editingFinished, e, [e, plug, is_int] {
        if (!e->property("editing").toBool()) return;
        e->setProperty("editing", false);
        exec("setAttr", args(QJsonArray{plug, is_int ? QJsonValue(qint64(e->text().toDouble())) : QJsonValue(e->text().toDouble())}));
    });
    h->addWidget(e);
    h->addWidget(s, 1);
    return row;
}

QWidget* attr_field(const QJsonObject& a, QWidget* parent) {
    QString type = a["type"].toString(), plug = a["plug"].toString();
    bool ro = a["output"].toBool() || a["connected"].toBool();
    if (type == "bool") {
        auto* cb = new QCheckBox(parent);
        cb->setChecked(a["value"].toBool());
        cb->setEnabled(!ro);
        QObject::connect(cb, &QCheckBox::toggled, cb, [plug](bool on) { set_plug(plug, on, "bool"); });
        return cb;
    }
    if (type == "enum") {
        auto* box = new QComboBox(parent);
        for (const QJsonValue& n : a["enum"].toArray()) box->addItem(n.toString());
        box->setCurrentIndex(a["value"].toInt());
        box->setEnabled(!ro);
        QObject::connect(box, &QComboBox::activated, box, [plug](int i) { set_plug(plug, i, "int"); });
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
            auto* e = new QLineEdit(fmt3(v.at(k).toDouble()), w);
            e->setReadOnly(ro);
            QString child = kids.at(k).toString();
            if (child.isEmpty()) child = a["attr"].toString() + QString("XYZ")[k];
            QString cp = plug.left(plug.lastIndexOf('.')) + "." + child;
            QObject::connect(e, &QLineEdit::textEdited, e, [e] { e->setProperty("editing", true); });
            QObject::connect(e, &QLineEdit::editingFinished, e, [e, cp] {
                if (!e->property("editing").toBool()) return;
                e->setProperty("editing", false);
                set_plug(cp, e->text().toDouble(), "float");
            });
            h->addWidget(e);
        }
        return w;
    }
    if (type == "float" || type == "int") {
        if (a["min"].isDouble() && a["max"].isDouble() && a["max"].toDouble() > a["min"].toDouble()) return ranged_field(a, a["min"].toDouble(), a["max"].toDouble(), parent);
        auto* e = new QLineEdit(type == "int" ? QString::number(a["value"].toInteger()) : fmt3(a["value"].toDouble()), parent);
        e->setReadOnly(ro);
        bool is_int = type == "int";
        QObject::connect(e, &QLineEdit::textEdited, e, [e] { e->setProperty("editing", true); });
        QObject::connect(e, &QLineEdit::editingFinished, e, [e, plug, is_int] {
            if (!e->property("editing").toBool()) return;
            e->setProperty("editing", false);
            set_plug(plug, is_int ? QJsonValue(qint64(e->text().toDouble())) : QJsonValue(e->text().toDouble()), "float");
        });
        return e;
    }
    if (type == "string") {
        auto* e = new QLineEdit(a["value"].toString(), parent);
        e->setReadOnly(ro);
        QObject::connect(e, &QLineEdit::textEdited, e, [e] { e->setProperty("editing", true); });
        QObject::connect(e, &QLineEdit::editingFinished, e, [e, plug] {
            if (!e->property("editing").toBool()) return;
            e->setProperty("editing", false);
            set_plug(plug, e->text(), "string");
        });
        return e;
    }
    return new QLabel(type == "matrix" ? "(matrix)" : "", parent);
}

// A collapsible section header, remembered per title across refreshes.
QSet<QString>& closed_sections() {
    static QSet<QString> s{"Node Behaviour", "Limit Information", "Extra Attributes"};
    return s;
}

QFormLayout* add_section(QVBoxLayout* v, QWidget* parent, const QString& title) {
    auto* head = new QToolButton(parent);
    head->setObjectName("aeSectionButton");
    head->setText(title);
    head->setCheckable(true);
    bool open = !closed_sections().contains(title);
    head->setChecked(open);
    head->setArrowType(open ? Qt::DownArrow : Qt::RightArrow);
    head->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    head->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    v->addWidget(head);
    auto* box = new QWidget(parent);
    auto* f = new QFormLayout(box);
    f->setLabelAlignment(Qt::AlignRight);
    f->setContentsMargins(8, 2, 2, 6);
    f->setVerticalSpacing(3);
    box->setVisible(open);
    v->addWidget(box);
    QObject::connect(head, &QToolButton::toggled, box, [box, head, title](bool on) {
        box->setVisible(on);
        head->setArrowType(on ? Qt::DownArrow : Qt::RightArrow);
        if (on) closed_sections().remove(title);
        else closed_sections().insert(title);
    });
    return f;
}

} // namespace

AttributeEditor::AttributeEditor(QWidget* parent, const QString& pinned) : Panel(parent), pinned_(pinned) {
    auto* lay = new QVBoxLayout(this);
    lay->setContentsMargins(2, 2, 2, 2);
    lay->setSpacing(2);
    // List, Selected, Focus, Attributes, as a row of buttons above the tabs.
    auto* top = new QHBoxLayout();
    auto* back = new QToolButton(this);
    back->setArrowType(Qt::LeftArrow);
    back->setToolTip("The node before this one in the chain");
    auto* fwd = new QToolButton(this);
    fwd->setArrowType(Qt::RightArrow);
    fwd->setToolTip("The node after this one in the chain");
    connect(back, &QToolButton::clicked, this, [this] { tabs_->setCurrentIndex(qMax(0, tabs_->currentIndex() - 1)); });
    connect(fwd, &QToolButton::clicked, this, [this] { tabs_->setCurrentIndex(qMin(tabs_->count() - 1, tabs_->currentIndex() + 1)); });
    top->addWidget(back);
    top->addWidget(fwd);
    top->addStretch(1);
    lay->addLayout(top);
    tabs_ = new QTabWidget(this);
    tabs_->setDocumentMode(true);
    tabs_->setUsesScrollButtons(true);
    lay->addWidget(tabs_, 1);
    connect(tabs_, &QTabWidget::currentChanged, this, [this](int i) {
        if (i >= 0) current_tab_ = tabs_->tabText(i);
    });
    auto* bottom = new QHBoxLayout();
    auto* sel = new QPushButton("Select", this);
    auto* load = new QPushButton("Load Attributes", this);
    auto* copy = new QPushButton("Copy Tab", this);
    bottom->addWidget(sel);
    bottom->addWidget(load);
    bottom->addWidget(copy);
    lay->addLayout(bottom);
    connect(sel, &QPushButton::clicked, this, [this] {
        QWidget* w = tabs_->currentWidget();
        if (w) exec("select", args(QJsonArray{w->property("nodePath").toString()}));
    });
    connect(load, &QPushButton::clicked, this, [this] { refresh(); });
    connect(copy, &QPushButton::clicked, this, [this] {
        QWidget* w = tabs_->currentWidget();
        if (!w) return;
        // A window of its own that stays on this node.
        auto* win = new AttributeEditor(nullptr, w->property("nodePath").toString());
        win->setAttribute(Qt::WA_DeleteOnClose);
        win->setWindowFlag(Qt::Window);
        win->setWindowTitle(tabs_->tabText(tabs_->currentIndex()));
        win->resize(380, 560);
        win->show();
    });
}

void AttributeEditor::refresh() {
    QJsonObject ae = query("attributeEditor", pinned_).toObject();
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
        QString node_type = t["type"].toString();
        auto* scroll = new QScrollArea();
        scroll->setWidgetResizable(true);
        scroll->setFrameShape(QFrame::NoFrame);
        auto* body = new QWidget();
        auto* v = new QVBoxLayout(body);
        v->setContentsMargins(6, 6, 6, 6);
        v->setSpacing(2);
        auto* head = new QHBoxLayout();
        head->addWidget(new QLabel(QString("<b>%1:</b>").arg(node_type), body));
        auto* name = new QLineEdit(t["name"].toString(), body);
        QString path = t["path"].toString();
        scroll->setProperty("nodePath", path);
        connect(name, &QLineEdit::editingFinished, name, [name, path] {
            if (name->text().trimmed() != path.section('|', -1)) exec("rename", args(QJsonArray{path, name->text().trimmed()}));
        });
        head->addWidget(name, 1);
        auto* focus = new QToolButton(body);
        focus->setIcon(icon("frameSel", 16));
        focus->setToolTip("Focus: select this node");
        connect(focus, &QToolButton::clicked, focus, [path] { exec("select", args(QJsonArray{path})); });
        head->addWidget(focus);
        v->addLayout(head);

        QJsonArray attrs = t["attrs"].toArray();
        QSet<QString> used;
        for (const Section& sec : sections_for(node_type)) {
            QStringList names = QString(sec.attrs).split(' ');
            QFormLayout* f = nullptr;
            for (const QString& n : names)
                for (const QJsonValue& av : attrs) {
                    QJsonObject a = av.toObject();
                    if (a["attr"].toString() != n || a["type"].toString() == "matrix") continue;
                    if (!f) f = add_section(v, body, sec.title);
                    used.insert(n);
                    QWidget* field = attr_field(a, body);
                    field->setToolTip(a["doc"].toString());
                    f->addRow(a["nice"].toString(), field);
                }
        }
        // The rest grouped by their own category, then Extra Attributes.
        QMap<QString, QFormLayout*> forms;
        QJsonValue notes;
        QString notes_plug = path + ".notes";
        for (const QJsonValue& av : attrs) {
            QJsonObject a = av.toObject();
            QString n = a["attr"].toString();
            if (n == "notes") {
                notes = a["value"];
                continue;
            }
            if (used.contains(n) || a["type"].toString() == "matrix") continue;
            QString cat = a["category"].toString();
            if (cat.isEmpty()) cat = sections_for(node_type).size() > 1 ? QString("Extra Attributes") : QString("%1 Attributes").arg(node_type.left(1).toUpper() + node_type.mid(1));
            if (!forms.contains(cat)) forms[cat] = add_section(v, body, cat);
            QWidget* field = attr_field(a, body);
            field->setToolTip(a["doc"].toString());
            forms[cat]->addRow(a["nice"].toString(), field);
        }
        // Notes: a string attribute added the first time it's written.
        auto* note_head = new QLabel("Notes: " + t["name"].toString(), body);
        note_head->setObjectName("aeSection");
        v->addWidget(note_head);
        auto* note = new QPlainTextEdit(notes.toString(), body);
        note->setFixedHeight(70);
        note->setProperty("editing", false);
        bool had = !notes.isUndefined();
        connect(note, &QPlainTextEdit::textChanged, note, [note] { note->setProperty("editing", true); });
        note->installEventFilter(this);
        note->setProperty("notesPlug", notes_plug);
        note->setProperty("notesNode", path);
        note->setProperty("notesHad", had);
        v->addWidget(note);
        v->addStretch(1);
        scroll->setWidget(body);
        int idx = tabs_->addTab(scroll, t["name"].toString());
        if (t["name"].toString() == keep) select = idx;
    }
    tabs_->setCurrentIndex(select);
    tabs_->blockSignals(false);
}

bool AttributeEditor::eventFilter(QObject* o, QEvent* e) {
    if (e->type() == QEvent::FocusOut) {
        auto* note = qobject_cast<QPlainTextEdit*>(o);
        if (note && note->property("editing").toBool()) {
            note->setProperty("editing", false);
            if (!note->property("notesHad").toBool()) {
                exec("addAttr", QJsonObject{{"args", QJsonArray{note->property("notesNode").toString()}}, {"longName", "notes"}, {"dataType", "string"}});
                note->setProperty("notesHad", true);
            }
            set_plug(note->property("notesPlug").toString(), note->toPlainText(), "string");
        }
    }
    return Panel::eventFilter(o, e);
}

} // namespace az
