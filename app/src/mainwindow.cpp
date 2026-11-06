#include "mainwindow.h"
#include "aztek.h"
#include "host.h"
#include "icons.h"
#include "markingmenu.h"
#include "panels.h"
#include "viewport.h"
#include "widgets.h"
#include "grapheditor.h"
#include "uveditor.h"
#include "toolsettings.h"

#include <QAction>
#include <cstdio>
#include <QActionGroup>
#include <QApplication>
#include <QCloseEvent>
#include <QComboBox>
#include <QDateTime>
#include <QDockWidget>
#include <QFileDialog>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenuBar>
#include <QMessageBox>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QPainter>
#include <QFile>
#include <QDialog>
#include <QSettings>
#include <QSpinBox>
#include <QSplitter>
#include <QStatusBar>
#include <QTabWidget>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QVBoxLayout>

namespace az {

namespace {

QJsonObject args(const QJsonArray& a) {
    QJsonObject o;
    o["args"] = a;
    return o;
}

bool has_command(const QString& name) {
    static QSet<QString> names;
    if (names.isEmpty())
        for (const QJsonValue& c : query("commands").toArray()) names.insert(c.toObject()["name"].toString());
    return names.contains(name);
}

bool typing_widget(QWidget* w) {
    return qobject_cast<QLineEdit*>(w) || qobject_cast<QPlainTextEdit*>(w) || qobject_cast<QAbstractSpinBox*>(w) || qobject_cast<QComboBox*>(w);
}

} // namespace

MainWindow::MainWindow() {
    setWindowTitle("Aztek");
    setDockOptions(QMainWindow::AnimatedDocks | QMainWindow::AllowTabbedDocks | QMainWindow::AllowNestedDocks);
    setCorner(Qt::BottomRightCorner, Qt::RightDockWidgetArea);
    setCorner(Qt::TopRightCorner, Qt::RightDockWidgetArea);

    panels_ = new PanelLayout(this);
    connect(panels_, &PanelLayout::rightClick, this, &MainWindow::marking_menu);

    build_status_line();
    build_shelf();
    build_toolbox();
    build_docks();
    build_bottom();
    build_menus();

    // The centre: model panels.
    setCentralWidget(panels_);

    connect(Host::get(), &Host::sceneChanged, this, &MainWindow::refresh_state);
    connect(Host::get(), &Host::logLines, this, [this](const QJsonArray& lines) {
        if (script_editor_) script_editor_->append(lines);
        // The command line's result field shows the latest result or error.
        for (const QJsonValue& v : lines) {
            QJsonObject l = v.toObject();
            int k = l["kind"].toInt();
            if (k == 1 || k == 2 || k == 4) command_line_->show_result(l["text"].toString(), k == 2);
        }
    });

    qApp->installEventFilter(this);
    apply_prefs();
    resize(1600, 980);
    refresh_state();
    update_title();
}

// ---------------------------------------------------------------------------------------------------------------------
// Menus

QAction* MainWindow::cmd(QMenu* m, const QString& label, const QString& command, const QJsonObject& call, const QString& shortcut, bool options) {
    bool ok = has_command(command);
    auto run = [this, command, call, options] {
        QJsonObject c = call;
        // An option box's remembered settings apply to the plain item too, as in Maya.
        if (options) {
            QJsonObject saved = OptionsDialog::saved(command);
            for (auto it = saved.begin(); it != saved.end(); ++it)
                if (!c.contains(it.key())) c[it.key()] = it.value();
        }
        last_command_ = command;
        last_call_ = c;
        exec(command, c);
    };
    QAction* a;
    if (options && ok) {
        a = new OptionRow(label, shortcut, run, [command, label] { OptionsDialog::open(command, label); }, m);
        m->addAction(a);
    } else {
        a = m->addAction(label);
        if (!shortcut.isEmpty()) a->setText(label + "\t" + shortcut);
        connect(a, &QAction::triggered, this, run);
    }
    a->setEnabled(ok);
    a->setStatusTip(ok ? QString("%1: %2").arg(label, command) : QString("%1: not available yet").arg(label));
    return a;
}

QAction* MainWindow::todo(QMenu* m, const QString& label) {
    QAction* a = m->addAction(label);
    a->setEnabled(false);
    a->setStatusTip(label + ": not available yet");
    return a;
}

QMenu* MainWindow::file_menu() {
    auto* m = new QMenu("File", this);
    m->addAction(icon("new"), "New Scene\tCtrl+N", [] { aztek_free(aztek_file("new", "")); Host::get()->poke(); });
    m->addAction(icon("open"), "Open Scene...\tCtrl+O", this, &MainWindow::open_scene);
    m->addAction(icon("save"), "Save Scene\tCtrl+S", [this] { save_scene(false); });
    m->addAction("Save Scene As...\tCtrl+Shift+S", [this] { save_scene(true); });
    m->addSeparator();
    m->addAction("Import...", [] {
        QString f = QFileDialog::getOpenFileName(nullptr, "Import", QString(), "Scenes and meshes (*.ma *.obj *.ply *.stl);;Maya ASCII (*.ma);;Meshes (*.obj *.ply *.stl)");
        if (f.isEmpty()) return;
        if (f.endsWith(".ma", Qt::CaseInsensitive)) {
            exec("file", QJsonObject{{"args", QJsonArray{f}}, {"import", true}});
            return;
        }
        QString c = f.endsWith(".ply", Qt::CaseInsensitive) ? "importPly" : f.endsWith(".stl", Qt::CaseInsensitive) ? "importStl" : "importObj";
        exec(c, args(QJsonArray{f}));
    });
    m->addAction("Export Selection...", [] {
        QString f = QFileDialog::getSaveFileName(nullptr, "Export Selection", "export.ma", "Maya ASCII (*.ma);;OBJ (*.obj);;PLY (*.ply);;STL (*.stl)");
        if (f.isEmpty()) return;
        if (f.endsWith(".ma", Qt::CaseInsensitive)) {
            exec("file", QJsonObject{{"args", QJsonArray{f}}, {"exportSelected", true}, {"type", "mayaAscii"}});
            return;
        }
        QString c = f.endsWith(".ply", Qt::CaseInsensitive) ? "exportPly" : f.endsWith(".stl", Qt::CaseInsensitive) ? "exportStl" : "exportObj";
        exec(c, args(QJsonArray{f}));
    });
    m->addSeparator();
    QMenu* recent = m->addMenu("Recent Files");
    connect(recent, &QMenu::aboutToShow, this, [this, recent] {
        recent->clear();
        for (const QString& f : QSettings().value("recentFiles").toStringList()) recent->addAction(f, [this, f] { open_file(f); });
        if (recent->isEmpty()) recent->addAction("(none)")->setEnabled(false);
    });
    m->addSeparator();
    m->addAction("Exit\tCtrl+Q", this, &QWidget::close);
    return m;
}

QMenu* MainWindow::edit_menu() {
    auto* m = new QMenu("Edit", this);
    m->addAction(icon("undo"), "Undo\tZ", [] { aztek_undo(); Host::get()->poke(); });
    m->addAction(icon("redo"), "Redo\tShift+Z", [] { aztek_redo(); Host::get()->poke(); });
    m->addAction("Repeat\tG", this, &MainWindow::repeat_last);
    m->addSeparator();
    cmd(m, "Delete", "delete", {}, "Delete");
    QMenu* by_type = m->addMenu("Delete by Type");
    cmd(by_type, "History", "delete", QJsonObject{{"constructionHistory", true}}, "Alt+Shift+D");
    QMenu* all_type = m->addMenu("Delete All by Type");
    all_type->addAction("History", [] {
        QJsonArray all;
        for (const QJsonValue& v : query("outliner").toArray()) all.append(v.toObject()["path"].toString());
        QJsonObject c = args(all);
        c["constructionHistory"] = true;
        exec("delete", c);
    });
    m->addSeparator();
    cmd(m, "Duplicate", "duplicate", {}, "Ctrl+D", true);
    cmd(m, "Duplicate with Transform", "duplicateWithTransform", {}, "Shift+D");
    cmd(m, "Instance", "instance", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Group", "group", {}, "Ctrl+G", true);
    cmd(m, "Ungroup", "ungroup", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Parent", "parent", {}, "P", true);
    cmd(m, "Unparent", "parent", QJsonObject{{"world", true}}, "Shift+P");
    return m;
}

QMenu* MainWindow::create_menu() {
    auto* m = new QMenu("Create", this);
    QMenu* polys = m->addMenu("Polygon Primitives");
    struct P { const char* label; const char* cmd; };
    for (P p : {P{"Sphere", "polySphere"}, P{"Cube", "polyCube"}, P{"Cylinder", "polyCylinder"}, P{"Cone", "polyCone"}, P{"Torus", "polyTorus"},
                P{"Plane", "polyPlane"}, P{"Disc", "polyDisc"}, P{"Pipe", "polyPipe"}, P{"Capsule", "polyCapsule"}, P{"Platonic Solid", "polyIcoSphere"},
                P{"Quad Sphere", "polyQuadSphere"}})
        cmd(polys, p.label, p.cmd, {}, QString(), true);
    todo(m, "NURBS Primitives");
    todo(m, "Lights");
    todo(m, "Cameras");
    todo(m, "Curve Tools");
    m->addSeparator();
    cmd(m, "Locator", "spaceLocator");
    cmd(m, "Empty Group", "group", QJsonObject{{"empty", true}}, "Ctrl+G");
    cmd(m, "Sets", "createSet", {}, QString(), true);
    return m;
}

QMenu* MainWindow::select_menu() {
    auto* m = new QMenu("Select", this);
    m->addAction("All\tCtrl+Shift+A", [] {
        QJsonArray all;
        for (const QJsonValue& v : query("outliner").toArray())
            if (v.toObject()["type"].toString() != "objectSet") all.append(v.toObject()["path"].toString());
        exec("select", args(all));
    });
    cmd(m, "Deselect All", "select", QJsonObject{{"clear", true}});
    cmd(m, "Hierarchy", "selectHierarchy");
    cmd(m, "Inverse", "invertSelection", {}, "Ctrl+Shift+I");
    m->addSeparator();
    cmd(m, "Grow", "growSelection", {}, ">");
    cmd(m, "Shrink", "shrinkSelection", {}, "<");
    m->addSeparator();
    cmd(m, "Select Edge Loop", "selectLoop");
    cmd(m, "Select Border Edges", "selectBorder");
    cmd(m, "Select Shell", "selectConnected");
    cmd(m, "Select Shortest Edge Path", "selectPathBetween");
    cmd(m, "Select by Angle", "selectByAngle", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Object/Component", "selectMode", args(QJsonArray{"object"}), "F8");
    cmd(m, "Vertex", "selectMode", args(QJsonArray{"vertex"}), "F9");
    cmd(m, "Edge", "selectMode", args(QJsonArray{"edge"}), "F10");
    cmd(m, "Face", "selectMode", args(QJsonArray{"face"}), "F11");
    m->addSeparator();
    QMenu* conv = m->addMenu("Convert Selection");
    cmd(conv, "To Vertices", "convertSelection", args(QJsonArray{"vertex"}), "Ctrl+F9");
    cmd(conv, "To Edges", "convertSelection", args(QJsonArray{"edge"}), "Ctrl+F10");
    cmd(conv, "To Faces", "convertSelection", args(QJsonArray{"face"}), "Ctrl+F11");
    return m;
}

QMenu* MainWindow::modify_menu() {
    auto* m = new QMenu("Modify", this);
    cmd(m, "Freeze Transformations", "freezeTransforms", {}, QString(), true);
    cmd(m, "Reset Transformations", "resetTransforms", {}, QString(), true);
    m->addSeparator();
    QMenu* pivot = m->addMenu("Pivot");
    cmd(pivot, "Center Pivot", "centerPivot");
    cmd(pivot, "Bake Pivot", "bakePivot");
    cmd(pivot, "Reset Pivot", "resetPivot");
    cmd(pivot, "Zero Pivot", "zeroPivot");
    cmd(m, "Match Pivots", "matchPivot");
    m->addSeparator();
    cmd(m, "Convert to Instance", "convertToInstance");
    cmd(m, "Make Unique", "makeUnique");
    m->addSeparator();
    cmd(m, "Soft Select", "softSelect", QJsonObject{{"softSelectEnabled", 1}}, "B", true);
    return m;
}

QMenu* MainWindow::display_menu() {
    auto* m = new QMenu("Display", this);
    QMenu* hide = m->addMenu("Hide");
    cmd(hide, "Hide Selection", "hide", {}, "Ctrl+H");
    QMenu* show = m->addMenu("Show");
    cmd(show, "Show Selection", "show");
    cmd(show, "Show Last Hidden", "showLastHidden", {}, "Ctrl+Shift+H");
    cmd(m, "Toggle Show/Hide", "toggleVisibility", {}, "H");
    m->addSeparator();
    QMenu* grid = m->addMenu("Grid");
    grid->addAction("Show", [this] { panels_->set_all("grid", "true"); });
    grid->addAction("Hide", [this] { panels_->set_all("grid", "false"); });
    m->addSeparator();
    QMenu* sp = m->addMenu("Smooth Preview");
    cmd(sp, "Off", "smoothPreview", QJsonObject{{"level", 0}}, "1");
    sp->addAction("Cage + Smooth\t2", [this] {
        for (ViewportWidget* vw : panels_->views()) vw->set("smoothCage", "true");
        exec("smoothPreview", QJsonObject{{"level", 2}});
    });
    sp->addAction("Smooth\t3", [this] {
        for (ViewportWidget* vw : panels_->views()) vw->set("smoothCage", "false");
        exec("smoothPreview", QJsonObject{{"level", 2}});
    });
    return m;
}

QMenu* MainWindow::windows_menu() {
    auto* m = new QMenu("Windows", this);
    QMenu* ge = m->addMenu("General Editors");
    ge->addAction("Script Editor", this, &MainWindow::show_script_editor);
    auto* anim_ed = m->addMenu("Animation Editors");
    anim_ed->addAction("Graph Editor", this, &MainWindow::show_graph_editor);
    auto* model_ed = m->addMenu("Modeling Editors");
    model_ed->addAction("UV Editor", this, &MainWindow::show_uv_editor);
    ge->addAction("Attribute Editor\tCtrl+A", this, &MainWindow::show_attribute_editor);
    ge->addAction("Channel Box / Layer Editor", [this] {
        channel_dock_->show();
        channel_dock_->raise();
    });
    m->addAction("Outliner", [this] {
        outliner_dock_->show();
        outliner_dock_->raise();
    });
    m->addSeparator();
    QMenu* layouts = m->addMenu("Workspaces");
    layouts->addAction("Single Perspective View", [this] { panels_->set_single(true); });
    layouts->addAction("Four View", [this] { panels_->set_single(false); });
    m->addSeparator();
    m->addAction("Tool Settings...\tCtrl+Shift+T", this, &MainWindow::show_tool_settings);
    QMenu* prefs = m->addMenu("Settings/Preferences");
    prefs->addAction("Preferences...", this, &MainWindow::show_preferences);
    prefs->addAction("Hotkey Editor...", [this] {
        QMessageBox::information(this, "Hotkeys",
                                 "Q W E R  select, move, rotate, scale\nF / A  frame selection / all\nSpace  tap: single/four views, hold: hotbox\n"
                                 "F8 F9 F10 F11  object/component, vertex, edge, face\nAlt + left/middle/right drag  tumble, track, dolly\n"
                                 "Right drag  marking menu; Shift+right: tools; Ctrl+right: convert\nZ / Shift+Z  undo / redo\nG  repeat last\n"
                                 "Ctrl+D  duplicate   Ctrl+G  group   P / Shift+P  parent / unparent\nCtrl+H / Ctrl+Shift+H  hide / show last hidden\n"
                                 "Ctrl+E  extrude   Ctrl+B  bevel   > <  grow / shrink\n1 2 3  smooth preview   4 5  wireframe / shaded\n"
                                 "+ -  manipulator size   Ctrl+A  attribute editor\nCtrl+N Ctrl+O Ctrl+S  new, open, save");
    });
    return m;
}

QMenu* MainWindow::mesh_menu() {
    auto* m = new QMenu("Mesh", this);
    QMenu* b = m->addMenu("Booleans");
    cmd(b, "Union", "polyBoolean", QJsonObject{{"operation", "union"}});
    cmd(b, "Difference", "polyBoolean", QJsonObject{{"operation", "difference"}});
    cmd(b, "Intersection", "polyBoolean", QJsonObject{{"operation", "intersection"}});
    cmd(m, "Combine", "polyCombine", {}, QString(), true);
    cmd(m, "Separate", "polySeparate");
    m->addSeparator();
    cmd(m, "Conform", "polyConformToObject", {}, QString(), true);
    cmd(m, "Fill Hole", "polyFillHole", {}, QString(), true);
    cmd(m, "Reduce", "polyReduce", {}, QString(), true);
    cmd(m, "Smooth", "polySubdivide", {}, QString(), true);
    cmd(m, "Average Vertices", "polyAverageVertex", {}, QString(), true);
    cmd(m, "Triangulate", "polyTriangulate", {}, QString(), true);
    cmd(m, "Quadrangulate", "polyQuadrangulate", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Mirror", "polyMirror", {}, QString(), true);
    cmd(m, "Symmetrize", "symmetrize", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Cleanup...", "meshCheck", {}, QString(), true);
    return m;
}

QMenu* MainWindow::edit_mesh_menu() {
    auto* m = new QMenu("Edit Mesh", this);
    QLabel* comp = new QLabel("  Components", m);
    comp->setObjectName("menuSection");
    cmd(m, "Bevel", "polyBevel", {}, "Ctrl+B", true);
    cmd(m, "Bridge", "polyBridge", {}, QString(), true);
    cmd(m, "Collapse", "polyCollapse");
    cmd(m, "Connect", "polyConnect");
    cmd(m, "Extrude", "polyExtrude", {}, "Ctrl+E", true);
    cmd(m, "Inset", "polyInset", {}, QString(), true);
    cmd(m, "Merge", "polyMerge", {}, QString(), true);
    cmd(m, "Merge to Center", "polyMergeCenter");
    m->addSeparator();
    cmd(m, "Flip", "polyFlip");
    cmd(m, "Spherize", "polySpherize", {}, QString(), true);
    cmd(m, "Circularize", "polyCircularize", {}, QString(), true);
    cmd(m, "Straighten", "polyStraighten", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Delete Edge/Vertex", "polyDissolve", {}, "Ctrl+Delete");
    cmd(m, "Edit Edge Flow", "polyEditEdgeFlow", {}, QString(), true);
    cmd(m, "Spin Edge", "polySpinEdge");
    cmd(m, "Poke Face", "polyPoke");
    cmd(m, "Duplicate", "polyDuplicateFaces");
    cmd(m, "Extract", "polyExtract", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Grid Fill", "polyGridFill", {}, QString(), true);
    cmd(m, "Split Rings", "polySplitRings", {}, QString(), true);
    return m;
}

QMenu* MainWindow::mesh_tools_menu() {
    auto* m = new QMenu("Mesh Tools", this);
    cmd(m, "Insert Edge Loop", "polyInsertLoop", {}, QString(), true);
    cmd(m, "Offset Edge Loop", "polyOffsetLoop", {}, QString(), true);
    cmd(m, "Multi-Cut", "polyKnife", {}, QString(), true);
    cmd(m, "Target Weld", "polyTargetWeld", {}, QString(), true);
    todo(m, "Quad Draw");
    todo(m, "Create Polygon");
    todo(m, "Append to Polygon");
    todo(m, "Slide Edge");
    m->addSeparator();
    cmd(m, "Symmetry: Toggle", "toggleSymmetry");
    cmd(m, "Symmetry: World X", "setSymmetry", QJsonObject{{"axis", "x"}});
    return m;
}

QMenu* MainWindow::mesh_display_menu() {
    auto* m = new QMenu("Mesh Display", this);
    cmd(m, "Conform", "polyConform");
    cmd(m, "Reverse", "polyFlip");
    m->addSeparator();
    cmd(m, "Soften Edge", "polySoftEdge", QJsonObject{{"angle", 180.0}});
    cmd(m, "Harden Edge", "polySoftEdge", QJsonObject{{"angle", 0.0}});
    cmd(m, "Soft/Hard Edges", "polySoftEdge", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Lock Normals", "polyLockNormals");
    cmd(m, "Unlock Normals", "polyUnlockNormals");
    cmd(m, "Face Weighted Normals", "polyWeightedNormals", {}, QString(), true);
    return m;
}

QMenu* MainWindow::uv_menu() {
    auto* m = new QMenu("UV", this);
    cmd(m, "Automatic", "polyAutoProjection", {}, QString(), true);
    cmd(m, "Planar", "polyProjection", QJsonObject{{"type", "planar"}}, QString(), true);
    cmd(m, "Cylindrical", "polyProjection", QJsonObject{{"type", "cylindrical"}});
    cmd(m, "Spherical", "polyProjection", QJsonObject{{"type", "spherical"}});
    m->addSeparator();
    cmd(m, "Unfold", "u3dUnfold", {}, QString(), true);
    cmd(m, "Layout", "u3dLayout", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Cut", "polyMapCut");
    cmd(m, "Sew", "polyMapSew");
    cmd(m, "Move and Sew", "polyMapSewMove");
    m->addSeparator();
    cmd(m, "Normalize", "polyNormalizeUV", {}, QString(), true);
    cmd(m, "Flip", "polyFlipUV", {}, QString(), true);
    m->addSeparator();
    m->addAction("UV Editor", this, &MainWindow::show_uv_editor);
    return m;
}

QMenu* MainWindow::deform_menu() {
    auto* m = new QMenu("Deform", this);
    cmd(m, "Blend Shape", "blendShape", {}, QString(), true);
    cmd(m, "Cluster", "cluster", {}, QString(), true);
    auto* nl = m->addMenu("Nonlinear");
    for (const char* t : {"bend", "flare", "sine", "squash", "twist", "wave"}) {
        QString label = QString(t);
        label[0] = label[0].toUpper();
        cmd(nl, label, "nonLinear", QJsonObject{{"type", t}}, QString(), true);
    }
    m->addSeparator();
    for (const char* l : {"Lattice", "Wrap", "Wire", "Soft Modification", "Delta Mush", "Tension"}) todo(m, l);
    return m;
}

QMenu* MainWindow::skeleton_menu() {
    auto* m = new QMenu("Skeleton", this);
    m->addAction("Create Joints", [this] { set_tool("joint"); });
    m->addAction("Orient Joint", [] { exec("joint", QJsonObject{{"edit", true}, {"orientJoint", "xyz"}, {"secondaryAxisOrient", "yup"}, {"children", true}}); });
    m->addSeparator();
    cmd(m, "Create IK Handle", "ikHandle", QJsonObject{{"solver", "ikRPsolver"}}, QString(), true);
    cmd(m, "Create IK Handle (Single Chain)", "ikHandle", QJsonObject{{"solver", "ikSCsolver"}});
    return m;
}

QMenu* MainWindow::skin_menu() {
    auto* m = new QMenu("Skin", this);
    cmd(m, "Bind Skin", "skinCluster", QJsonObject{{"toSelectedBones", false}}, QString(), true);
    cmd(m, "Unbind Skin", "skinCluster", QJsonObject{{"edit", true}, {"unbind", true}});
    cmd(m, "Go to Bind Pose", "dagPose", QJsonObject{{"restore", true}, {"global", true}, {"bindPose", true}});
    m->addSeparator();
    m->addAction("Paint Skin Weights", [this] { set_tool("paintSkin"); show_tool_settings(); });
    // Mirror and copy work on the selection: one skinned mesh mirrors onto itself, two copy from the first to the second.
    QMenu* mirror = m->addMenu("Mirror Skin Weights");
    for (auto [label, plane, inv] : {std::tuple{"Across YZ, +X to -X", "YZ", false}, {"Across YZ, -X to +X", "YZ", true}, {"Across XZ, +Y to -Y", "XZ", false}, {"Across XY, +Z to -Z", "XY", false}}) {
        QString pl = plane;
        bool in = inv;
        mirror->addAction(label, [pl, in] {
            QString obj = state()["selection"].toArray().at(0).toString().section('.', 0, 0);
            QString sk = ask("findRelatedSkinCluster", QJsonObject{{"args", QJsonArray{obj}}}).toString();
            if (sk.isEmpty()) { log("// Warning: select a skinned mesh //", 4); return; }
            exec("copySkinWeights", QJsonObject{{"sourceSkin", sk}, {"destinationSkin", sk}, {"mirrorMode", pl}, {"mirrorInverse", in}, {"influenceAssociation", QJsonArray{"label", "closestJoint"}}});
        });
    }
    m->addAction("Copy Skin Weights", [] {
        QJsonArray sel = state()["selection"].toArray();
        if (sel.size() < 2) { log("// Warning: select the source mesh, then the destination //", 4); return; }
        exec("copySkinWeights", QJsonObject{{"args", QJsonArray{sel.at(0), sel.at(sel.size() - 1)}}, {"noMirror", true}, {"influenceAssociation", QJsonArray{"closestJoint", "oneToOne"}}});
    });
    auto on_selected = [](const QString& what, std::function<QJsonObject()> call, const QString& command) {
        return [what, call, command] {
            QString obj = state()["selection"].toArray().at(0).toString().section('.', 0, 0);
            QString sk = ask("findRelatedSkinCluster", QJsonObject{{"args", QJsonArray{obj}}}).toString();
            if (sk.isEmpty()) { log("// Warning: select a skinned mesh //", 4); return; }
            QJsonObject c = call();
            c["args"] = QJsonArray{sk};
            exec(command, c);
        };
    };
    m->addAction("Smooth Skin Weights", on_selected("smooth", [] { return QJsonObject{{"edit", true}, {"smoothWeights", 0.5}, {"smoothWeightsMaxIterations", 2}}; }, "skinCluster"));
    m->addAction("Prune Small Weights", on_selected("prune", [] { return QJsonObject{{"pruneWeights", 0.01}}; }, "skinPercent"));
    m->addAction("Normalize Weights", on_selected("normalize", [] { return QJsonObject{{"edit", true}, {"forceNormalizeWeights", true}}; }, "skinCluster"));
    m->addAction("Reset Weights to Default", on_selected("reset", [] { return QJsonObject{{"resetToDefault", true}}; }, "skinPercent"));
    m->addSeparator();
    // Selection order: the skinned mesh and the joints.
    auto influence = [](const char* flag) {
        QString fl = flag;
        return [fl] {
            QString sk;
            QJsonArray joints;
            for (const QJsonValue& s : state()["selection"].toArray()) {
                QString found = ask("findRelatedSkinCluster", QJsonObject{{"args", QJsonArray{s.toString().section('.', 0, 0)}}}).toString();
                if (!found.isEmpty()) sk = found;
                else joints.append(s);
            }
            if (sk.isEmpty() || joints.isEmpty()) { log("// Warning: select a skinned mesh and joints //", 4); return; }
            QJsonObject c{{"args", QJsonArray{sk}}, {"edit", true}};
            c[fl] = joints;
            exec("skinCluster", c);
        };
    };
    m->addAction("Add Influence", influence("addInfluence"));
    m->addAction("Remove Influence", influence("removeInfluence"));
    return m;
}

QMenu* MainWindow::constrain_menu() {
    auto* m = new QMenu("Constrain", this);
    cmd(m, "Parent", "parentConstraint", {}, QString(), true);
    cmd(m, "Point", "pointConstraint", {}, QString(), true);
    cmd(m, "Orient", "orientConstraint", {}, QString(), true);
    cmd(m, "Scale", "scaleConstraint", {}, QString(), true);
    cmd(m, "Aim", "aimConstraint", {}, QString(), true);
    return m;
}

QMenu* MainWindow::control_menu() {
    auto* m = new QMenu("Control", this);
    cmd(m, "Add Attribute...", "addAttr", {}, QString(), true);
    cmd(m, "Delete Attribute", "deleteAttr");
    cmd(m, "Connect Attribute", "connectAttr", {}, QString(), true);
    return m;
}

QMenu* MainWindow::key_menu() {
    auto* m = new QMenu("Key", this);
    cmd(m, "Set Key", "setKeyframe", {}, "S", true);
    auto* tk = m->addMenu("Set Transform Keys");
    cmd(tk, "Translate", "setKeyframe", QJsonObject{{"attribute", "translate"}}, "Shift+W");
    cmd(tk, "Rotate", "setKeyframe", QJsonObject{{"attribute", "rotate"}}, "Shift+E");
    cmd(tk, "Scale", "setKeyframe", QJsonObject{{"attribute", "scale"}}, "Shift+R");
    m->addAction("Auto Key", [this](bool) { auto_key_->trigger(); });
    m->addSeparator();
    cmd(m, "Delete Keys", "cutKey", {}, QString(), true);
    m->addAction("Delete Keys at Current Frame", [this] {
        exec("cutKey", QJsonObject{{"time", time_slider_->time()}});
    });
    m->addSeparator();
    auto* tan = m->addMenu("Tangents");
    for (const char* t : {"auto", "spline", "clamped", "linear", "flat", "step", "plateau"}) {
        QString label = QString(t);
        label[0] = label[0].toUpper();
        cmd(tan, label, "keyTangent", QJsonObject{{"inTangentType", t == QString("step") ? "clamped" : t}, {"outTangentType", t}});
    }
    auto* def = m->addMenu("Default Tangents");
    for (const char* t : {"auto", "spline", "clamped", "linear", "flat", "step", "plateau"}) {
        QString label = QString(t);
        label[0] = label[0].toUpper();
        cmd(def, label, "keyTangent", QJsonObject{{"global", true}, {"inTangentType", t == QString("step") ? "clamped" : t}, {"outTangentType", t}});
    }
    m->addSeparator();
    m->addAction("Graph Editor...", [this] { show_graph_editor(); });
    return m;
}

QMenu* MainWindow::playback_menu() {
    auto* m = new QMenu("Playback", this);
    m->addAction("Play / Stop\tAlt+V", [this] { toggle_play(1); });
    m->addAction("Play Backwards", [this] { toggle_play(-1); });
    m->addSeparator();
    m->addAction("Go to Start of Playback Range", [this] { time_slider_->set_time(time_slider_->start()); });
    m->addAction("Go to End of Playback Range", [this] { time_slider_->set_time(time_slider_->end()); });
    m->addAction("Next Frame\tAlt+.", [this] { time_slider_->set_time(time_slider_->time() + 1); });
    m->addAction("Previous Frame\tAlt+,", [this] { time_slider_->set_time(time_slider_->time() - 1); });
    m->addAction("Next Key\t.", [this] {
        QJsonValue f = ask("findKeyframe", QJsonObject{{"which", "next"}});
        if (f.isDouble()) time_slider_->set_time(f.toDouble());
    });
    m->addAction("Previous Key\t,", [this] {
        QJsonValue f = ask("findKeyframe", QJsonObject{{"which", "previous"}});
        if (f.isDouble()) time_slider_->set_time(f.toDouble());
    });
    return m;
}

void MainWindow::build_menus() {
    QMenuBar* mb = menuBar();
    mb->setNativeMenuBar(false);
    build_menu_set(menu_set_->currentText());
}

void MainWindow::build_menu_set(const QString& set) {
    QMenuBar* mb = menuBar();
    mb->clear();
    for (QMenu* m : set_menus_) m->deleteLater();
    set_menus_.clear();
    QList<QMenu*> menus = {file_menu(), edit_menu(), create_menu(), select_menu(), modify_menu(), display_menu(), windows_menu()};
    if (set == "Modeling")
        menus << mesh_menu() << edit_mesh_menu() << mesh_tools_menu() << mesh_display_menu() << uv_menu() << deform_menu();
    else if (set == "Rigging")
        menus << skeleton_menu() << skin_menu() << deform_menu() << constrain_menu() << control_menu();
    else
        menus << key_menu() << playback_menu() << deform_menu() << constrain_menu();
    for (QMenu* m : menus) {
        m->setToolTipsVisible(true);
        mb->addMenu(m);
        set_menus_.append(m);
    }
    auto* help = new QMenu("Help", this);
    help->addAction("About Aztek", [this] { show_about(this); });
    mb->addMenu(help);
    set_menus_.append(help);
    if (hotbox_) hotbox_->deleteLater();
    hotbox_ = new Hotbox(set_menus_, this);
}

// ---------------------------------------------------------------------------------------------------------------------
// Status line, shelf, toolbox

void MainWindow::build_status_line() {
    QToolBar* tb = addToolBar("Status Line");
    tb->setObjectName("statusLine");
    tb->setMovable(false);
    tb->setIconSize(QSize(20, 20));
    menu_set_ = new QComboBox(tb);
    menu_set_->addItems({"Modeling", "Rigging", "Animation"});
    menu_set_->setToolTip("Menu set");
    connect(menu_set_, &QComboBox::currentTextChanged, this, &MainWindow::build_menu_set);
    tb->addWidget(menu_set_);
    tb->addSeparator();
    tb->addAction(icon("new"), "New Scene", [] { aztek_free(aztek_file("new", "")); Host::get()->poke(); });
    tb->addAction(icon("open"), "Open Scene", this, &MainWindow::open_scene);
    tb->addAction(icon("save"), "Save Scene", [this] { save_scene(false); });
    tb->addSeparator();
    tb->addAction(icon("undo"), "Undo", [] { aztek_undo(); Host::get()->poke(); });
    tb->addAction(icon("redo"), "Redo", [] { aztek_redo(); Host::get()->poke(); });
    tb->addSeparator();
    auto* modes = new QActionGroup(this);
    for (const char* m : {"object", "vertex", "edge", "face"}) {
        QString mode = m;
        QAction* a = tb->addAction(icon(mode), QString("Select by %1").arg(mode == "object" ? "object type" : mode));
        a->setCheckable(true);
        modes->addAction(a);
        connect(a, &QAction::triggered, this, [this, mode] { set_mode(mode); });
        mode_buttons_[mode] = a;
    }
    tb->addSeparator();
    history_button_ = tb->addAction(icon("history"), "Construction history on/off");
    history_button_->setCheckable(true);
    connect(history_button_, &QAction::triggered, this, [](bool on) { exec("constructionHistory", QJsonObject{{"state", on}}); });
    tb->addSeparator();
    QAction* sym = tb->addAction(icon("mirror"), "Symmetry (toggle on the selected mesh)");
    connect(sym, &QAction::triggered, this, [] { exec("toggleSymmetry"); });
}

void MainWindow::build_shelf() {
    addToolBarBreak();
    QToolBar* holder = addToolBar("Shelf");
    holder->setObjectName("shelf");
    holder->setMovable(false);
    auto* tabs = new QTabWidget(holder);
    tabs->setObjectName("shelfTabs");
    tabs->setDocumentMode(true);
    auto shelf = [&](const QString& name, std::function<void(QToolBar*)> fill) {
        auto* t = new QToolBar(tabs);
        t->setIconSize(QSize(32, 32));
        t->setToolButtonStyle(Qt::ToolButtonIconOnly);
        fill(t);
        tabs->addTab(t, name);
    };
    auto btn = [this](QToolBar* t, const QString& ic, const QString& tip, const QString& command, const QJsonObject& call = QJsonObject()) {
        QAction* a = t->addAction(icon(ic), tip, [this, command, call] {
            QJsonObject c = call;
            QJsonObject saved = OptionsDialog::saved(command);
            for (auto it = saved.begin(); it != saved.end(); ++it)
                if (!c.contains(it.key())) c[it.key()] = it.value();
            last_command_ = command;
            last_call_ = c;
            exec(command, c);
        });
        a->setEnabled(has_command(command));
        a->setStatusTip(tip);
    };
    shelf("Poly Modeling", [&](QToolBar* t) {
        btn(t, "sphere", "Polygon Sphere", "polySphere");
        btn(t, "cube", "Polygon Cube", "polyCube");
        btn(t, "cylinder", "Polygon Cylinder", "polyCylinder");
        btn(t, "cone", "Polygon Cone", "polyCone");
        btn(t, "torus", "Polygon Torus", "polyTorus");
        btn(t, "plane", "Polygon Plane", "polyPlane");
        btn(t, "disc", "Polygon Disc", "polyDisc");
        btn(t, "pipe", "Polygon Pipe", "polyPipe");
        t->addSeparator();
        btn(t, "combine", "Combine", "polyCombine");
        btn(t, "separate", "Separate", "polySeparate");
        btn(t, "smooth", "Smooth", "polySubdivide");
        btn(t, "boolean", "Boolean Union", "polyBoolean", QJsonObject{{"operation", "union"}});
        btn(t, "mirror", "Mirror", "polyMirror");
        t->addSeparator();
        btn(t, "extrude", "Extrude", "polyExtrude");
        btn(t, "bridge", "Bridge", "polyBridge");
        btn(t, "bevel", "Bevel", "polyBevel");
        btn(t, "insertLoop", "Insert Edge Loop", "polyInsertLoop");
        btn(t, "offsetLoop", "Offset Edge Loop", "polyOffsetLoop");
        btn(t, "merge", "Merge", "polyMerge");
        btn(t, "collapse", "Merge to Center", "polyMergeCenter");
    });
    shelf("General", [&](QToolBar* t) {
        btn(t, "locator", "Locator", "spaceLocator");
        btn(t, "group", "Group", "group");
        btn(t, "history", "Delete History", "delete", QJsonObject{{"constructionHistory", true}});
        btn(t, "transform", "Freeze Transformations", "freezeTransforms");
        btn(t, "frameSel", "Center Pivot", "centerPivot");
        btn(t, "layer", "New Display Layer", "createDisplayLayer");
    });
    shelf("Rigging", [&](QToolBar* t) {
        t->addAction(icon("joint"), "Create Joints", [this] { set_tool("joint"); });
        btn(t, "ik", "IK Handle", "ikHandle");
        btn(t, "skin", "Bind Skin", "skinCluster");
        t->addSeparator();
        btn(t, "parentCon", "Parent Constraint", "parentConstraint", QJsonObject{{"maintainOffset", true}});
        btn(t, "pointCon", "Point Constraint", "pointConstraint");
        btn(t, "orientCon", "Orient Constraint", "orientConstraint");
        t->addSeparator();
        btn(t, "locator", "Locator", "spaceLocator");
        btn(t, "group", "Group", "group");
    });
    shelf("Animation", [&](QToolBar* t) {
        btn(t, "key", "Set Key", "setKeyframe");
        t->addAction(icon("graph"), "Graph Editor", [this] { show_graph_editor(); });
        btn(t, "cutKey", "Delete Keys", "cutKey");
    });
    shelf("UV Editing", [&](QToolBar* t) {
        t->addAction(icon("uvEditor"), "UV Editor", [this] { show_uv_editor(); });
        btn(t, "uvAuto", "Automatic Projection", "polyAutoProjection");
        btn(t, "uvPlanar", "Planar Projection", "polyProjection", QJsonObject{{"type", "planar"}});
        btn(t, "uvUnfold", "Unfold", "u3dUnfold");
        btn(t, "uvLayout", "Layout", "u3dLayout");
        btn(t, "uvCut", "Cut UV Edges", "polyMapCut");
        btn(t, "uvSew", "Sew UV Edges", "polyMapSew");
    });
    // Custom: MEL buttons the user adds from the right-click menu, remembered between sessions.
    auto* custom = new QToolBar(tabs);
    custom->setIconSize(QSize(32, 32));
    custom->setToolButtonStyle(Qt::ToolButtonTextUnderIcon);
    auto load_custom = [custom] {
        custom->clear();
        QSettings s;
        int n = s.beginReadArray("customShelf");
        for (int i = 0; i < n; ++i) {
            s.setArrayIndex(i);
            QString label = s.value("label").toString(), code = s.value("code").toString();
            QAction* a = custom->addAction(icon(label), label, [code] { mel(code); });
            a->setToolTip(code);
        }
        s.endArray();
    };
    load_custom();
    custom->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(custom, &QWidget::customContextMenuRequested, this, [this, custom, load_custom](const QPoint& p) {
        QMenu m;
        m.addAction("Add MEL Button...", [this, load_custom] {
            QDialog d(this);
            d.setWindowTitle("Add Shelf Button");
            auto* lay = new QVBoxLayout(&d);
            auto* label = new QLineEdit(&d);
            label->setPlaceholderText("Label");
            auto* code = new QPlainTextEdit(&d);
            code->setPlaceholderText("MEL, e.g. polyCube -w 2; polyExtrude;");
            auto* ok = new QPushButton("Add", &d);
            lay->addWidget(label);
            lay->addWidget(code);
            lay->addWidget(ok);
            connect(ok, &QPushButton::clicked, &d, &QDialog::accept);
            if (d.exec() != QDialog::Accepted || label->text().isEmpty()) return;
            QSettings s;
            QList<QPair<QString, QString>> items;
            int n = s.beginReadArray("customShelf");
            for (int i = 0; i < n; ++i) {
                s.setArrayIndex(i);
                items.append({s.value("label").toString(), s.value("code").toString()});
            }
            s.endArray();
            items.append({label->text(), code->toPlainText()});
            s.beginWriteArray("customShelf");
            for (int i = 0; i < items.size(); ++i) {
                s.setArrayIndex(i);
                s.setValue("label", items[i].first);
                s.setValue("code", items[i].second);
            }
            s.endArray();
            load_custom();
        });
        m.addAction("Clear Shelf", [load_custom] {
            QSettings().remove("customShelf");
            load_custom();
        });
        m.exec(custom->mapToGlobal(p));
    });
    tabs->addTab(custom, "Custom");
    tabs->setCurrentIndex(0);
    holder->addWidget(tabs);
}

void MainWindow::build_toolbox() {
    QToolBar* tb = new QToolBar("Toolbox", this);
    tb->setObjectName("toolbox");
    tb->setMovable(false);
    tb->setIconSize(QSize(26, 26));
    addToolBar(Qt::LeftToolBarArea, tb);
    auto* tools = new QActionGroup(this);
    struct T { const char* id; const char* ic; const char* tip; };
    for (T t : {T{"select", "select", "Select Tool (Q)"}, T{"move", "move", "Move Tool (W)"}, T{"rotate", "rotate", "Rotate Tool (E)"}, T{"scale", "scale", "Scale Tool (R)"}}) {
        QString id = t.id;
        QAction* a = tb->addAction(icon(t.ic), t.tip, [this, id] { set_tool(id); });
        a->setCheckable(true);
        tools->addAction(a);
        tool_buttons_[id] = a;
    }
    tool_buttons_["select"]->setChecked(true);
    tb->addSeparator();
    tb->addAction(icon("single"), "Single Perspective View", [this] { panels_->set_single(true); });
    tb->addAction(icon("four"), "Four View", [this] { panels_->set_single(false); });
    tb->addAction(icon("perspOutliner"), "Perspective/Outliner", [this] {
        panels_->set_single(true);
        outliner_dock_->show();
    });
}

void MainWindow::build_docks() {
    outliner_dock_ = new QDockWidget("Outliner", this);
    outliner_dock_->setObjectName("outlinerDock");
    outliner_dock_->setWidget(new Outliner(outliner_dock_));
    addDockWidget(Qt::LeftDockWidgetArea, outliner_dock_);
    resizeDocks({outliner_dock_}, {220}, Qt::Horizontal);

    channel_dock_ = new QDockWidget("Channel Box / Layer Editor", this);
    channel_dock_->setObjectName("channelDock");
    auto* split = new QSplitter(Qt::Vertical, channel_dock_);
    split->addWidget(new ChannelBox(split));
    auto* layers = new QWidget(split);
    auto* ll = new QVBoxLayout(layers);
    ll->setContentsMargins(0, 0, 0, 0);
    auto* tabs = new QTabWidget(layers);
    tabs->setDocumentMode(true);
    tabs->addTab(new LayerEditor(tabs), "Display");
    ll->addWidget(tabs);
    split->addWidget(layers);
    split->setStretchFactor(0, 3);
    split->setStretchFactor(1, 1);
    channel_dock_->setWidget(split);
    addDockWidget(Qt::RightDockWidgetArea, channel_dock_);

    attr_dock_ = new QDockWidget("Attribute Editor", this);
    attr_dock_->setObjectName("attrDock");
    attr_dock_->setWidget(new AttributeEditor(attr_dock_));
    addDockWidget(Qt::RightDockWidgetArea, attr_dock_);
    tabifyDockWidget(channel_dock_, attr_dock_);
    channel_dock_->raise();
    resizeDocks({channel_dock_}, {290}, Qt::Horizontal);

    tool_dock_ = new QDockWidget("Tool Settings", this);
    tool_dock_->setObjectName("toolDock");
    tool_settings_ = new ToolSettings(tool_dock_);
    tool_dock_->setWidget(tool_settings_);
    addDockWidget(Qt::LeftDockWidgetArea, tool_dock_);
    tabifyDockWidget(outliner_dock_, tool_dock_);
    tool_dock_->hide();
}

void MainWindow::build_bottom() {
    QToolBar* cl = new QToolBar("Command Line", this);
    cl->setObjectName("commandLine");
    cl->setMovable(false);
    addToolBar(Qt::BottomToolBarArea, cl);
    command_line_ = new CommandLine(cl);
    connect(command_line_, &CommandLine::openScriptEditor, this, &MainWindow::show_script_editor);
    cl->addWidget(command_line_);

    addToolBarBreak(Qt::BottomToolBarArea);
    QToolBar* bottom = new QToolBar("Time Slider", this);
    bottom->setObjectName("timeBar");
    bottom->setMovable(false);
    addToolBar(Qt::BottomToolBarArea, bottom);
    auto* w = new QWidget(bottom);
    auto* v = new QVBoxLayout(w);
    v->setContentsMargins(0, 0, 0, 0);
    v->setSpacing(2);
    auto* row1 = new QHBoxLayout();
    time_slider_ = new TimeSlider(w);
    auto* current = new QSpinBox(w);
    current_frame_ = current;
    current->setRange(-100000, 100000);
    current->setValue(1);
    current->setFixedWidth(64);
    connect(time_slider_, &TimeSlider::timeChanged, current, [current](double t) {
        current->blockSignals(true);
        current->setValue(int(t));
        current->blockSignals(false);
        exec_quiet("currentTime", QJsonObject{{"args", QJsonArray{t}}});
        Host::get()->poke();
    });
    connect(current, &QSpinBox::valueChanged, time_slider_, [this](int t) { time_slider_->set_time(t); });
    row1->addWidget(time_slider_, 1);
    row1->addWidget(current);
    auto* play = new QToolBar(w);
    play->setIconSize(QSize(16, 16));
    play_timer_ = new QTimer(this);
    play_timer_->setInterval(1000 / 24);
    connect(play_timer_, &QTimer::timeout, this, [this] {
        double t = time_slider_->time() + play_dir_;
        if (t > time_slider_->end()) t = time_slider_->start();
        if (t < time_slider_->start()) t = time_slider_->end();
        time_slider_->set_time(t);
    });
    auto key_step = [this](const QString& which) {
        QJsonValue f = ask("findKeyframe", QJsonObject{{"which", which}});
        if (f.isDouble()) time_slider_->set_time(f.toDouble());
    };
    play->addAction(icon("first"), "Go to start of playback range", [this] { time_slider_->set_time(time_slider_->start()); });
    play->addAction(icon("prev"), "Step back one frame", [this] { time_slider_->set_time(time_slider_->time() - 1); });
    play->addAction(icon("prevKey"), "Step back one key", [key_step] { key_step("previous"); });
    play->addAction(icon("playBack"), "Play backwards", [this] { toggle_play(-1); });
    play->addAction(icon("play"), "Play forwards", [this] { toggle_play(1); });
    play->addAction(icon("nextKey"), "Step forward one key", [key_step] { key_step("next"); });
    play->addAction(icon("next"), "Step forward one frame", [this] { time_slider_->set_time(time_slider_->time() + 1); });
    play->addAction(icon("last"), "Go to end of playback range", [this] { time_slider_->set_time(time_slider_->end()); });
    row1->addWidget(play);
    v->addLayout(row1);
    auto* row2 = new QHBoxLayout();
    auto field = [w](double val) {
        auto* s = new QSpinBox(w);
        s->setRange(-100000, 100000);
        s->setValue(int(val));
        s->setFixedWidth(64);
        return s;
    };
    anim_start_ = field(1), play_start_ = field(1), play_end_ = field(120), anim_end_ = field(200);
    range_slider_ = new RangeSlider(w);
    auto push = [this] {
        exec_quiet("playbackOptions", QJsonObject{{"animationStartTime", anim_start_->value()},
                                                  {"minTime", play_start_->value()},
                                                  {"maxTime", play_end_->value()},
                                                  {"animationEndTime", anim_end_->value()}});
        sync_time_range();
    };
    for (QSpinBox* sb : {anim_start_, play_start_, play_end_, anim_end_}) connect(sb, &QSpinBox::editingFinished, this, push);
    connect(range_slider_, &RangeSlider::rangeChanged, this, [this](double a, double b) {
        exec_quiet("playbackOptions", QJsonObject{{"minTime", a}, {"maxTime", b}});
        sync_time_range();
    });
    row2->addWidget(anim_start_);
    row2->addWidget(play_start_);
    row2->addWidget(range_slider_, 1);
    row2->addWidget(play_end_);
    row2->addWidget(anim_end_);
    auto* fps = new QComboBox(w);
    fps->addItems({"24 fps", "25 fps", "30 fps", "48 fps", "50 fps", "60 fps"});
    connect(fps, &QComboBox::currentTextChanged, this, [](const QString& t) {
        exec("playbackOptions", QJsonObject{{"framesPerSecond", t.split(' ')[0].toDouble()}});
    });
    row2->addWidget(fps);
    auto* tb = new QToolBar(w);
    tb->setIconSize(QSize(16, 16));
    auto_key_ = tb->addAction(icon("autokey"), "Auto keyframe toggle", [this](bool on) {
        exec("autoKeyframe", QJsonObject{{"state", on}});
    });
    auto_key_->setCheckable(true);
    auto_key_->setObjectName("autoKey");
    row2->addWidget(tb);
    v->addLayout(row2);
    bottom->addWidget(w);

    help_line_ = new QLabel(this);
    statusBar()->addWidget(help_line_, 1);
    statusBar()->setObjectName("helpLine");
    statusBar()->setSizeGripEnabled(false);
}

void MainWindow::apply_prefs() {
    QSettings s;
    QFont f = qApp->font();
    f.setPointSize(s.value("prefs/fontSize", 9).toInt());
    qApp->setFont(f);
    statusBar()->setVisible(s.value("prefs/helpLine", true).toBool());
    double bg = s.value("prefs/background", 0.36).toDouble();
    double manip = s.value("prefs/manipSize", 1.0).toDouble();
    for (ViewportWidget* v : panels_->views()) {
        v->set("background", QString::number(bg));
        v->set("manipSize", QString::number(manip));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// State

void MainWindow::toggle_play(int dir) {
    if (play_timer_->isActive() && play_dir_ == dir) {
        play_timer_->stop();
        return;
    }
    play_dir_ = dir;
    double fps = ask("playbackOptions", QJsonObject{{"query", true}, {"queried", QJsonArray{"framesPerSecond"}}}).toDouble(24);
    play_timer_->setInterval(int(1000 / qMax(1.0, fps)));
    play_timer_->start();
}

void MainWindow::stop_play() {
    play_timer_->stop();
}

void MainWindow::sync_time_range() {
    auto q = [](const char* flag) {
        return ask("playbackOptions", QJsonObject{{"query", true}, {"queried", QJsonArray{flag}}}).toDouble();
    };
    double ast = q("animationStartTime"), mn = q("minTime"), mx = q("maxTime"), aet = q("animationEndTime");
    for (auto [sb, val] : {std::pair{anim_start_, ast}, {play_start_, mn}, {play_end_, mx}, {anim_end_, aet}}) {
        if (sb->hasFocus()) continue;
        sb->blockSignals(true);
        sb->setValue(int(val));
        sb->blockSignals(false);
    }
    range_slider_->set(ast, mn, mx, aet);
    time_slider_->set_range(mn, mx);
}

void MainWindow::refresh_state() {
    QJsonObject st = state();
    for (const QJsonValue& r : query("uiRequests").toArray()) {
        QString n = r.toString();
        if (n == "GraphEditor") show_graph_editor();
        else if (n == "ScriptEditor") show_script_editor();
        else if (n == "OutlinerWindow") { outliner_dock_->show(); outliner_dock_->raise(); }
        else if (n == "AttributeEditor") show_attribute_editor();
        else if (n == "ToggleChannelsLayers") channel_dock_->setVisible(!channel_dock_->isVisible());
        else if (n == "FrameSelected") { if (active_view()) active_view()->set("frame", "\"selection\""); }
        else if (n == "FrameAll") { if (active_view()) active_view()->set("frame", "\"all\""); }
        else if (n == "PlaybackToggle") toggle_play(1);
        else if (n == "TextureViewWindow" || n == "UVEditor") show_uv_editor();
        else if (n == "PreferencesWindow") show_preferences();
        else if (n == "JointTool") set_tool("joint");
        else if (n == "ArtPaintSkinWeightsTool") set_tool("paintSkin");
        else if (n == "ArtPaintSkinWeightsToolOptions") { set_tool("paintSkin"); show_tool_settings(); }
        else if (n == "ToolSettingsWindow") show_tool_settings();
        else if (n == "SelectTool") set_tool("select");
        else if (n == "MoveTool") set_tool("move");
        else if (n == "RotateTool") set_tool("rotate");
        else if (n == "ScaleTool") set_tool("scale");
        else if (n == "FourViewLayout") panels_->set_single(false);
        else if (n == "SingleViewLayout") panels_->set_single(true);
    }
    // The time: scripts may have moved it, changed the range or set keys.
    double now = ask("currentTime", QJsonObject{{"query", true}}).toDouble(1);
    sync_time_range();
    if (now != time_slider_->time() && !play_timer_->isActive()) {
        time_slider_->blockSignals(true);
        time_slider_->set_time(now);
        time_slider_->blockSignals(false);
        current_frame_->blockSignals(true);
        current_frame_->setValue(int(now));
        current_frame_->blockSignals(false);
    }
    QJsonValue keys = ask("keyTimes");
    QVector<double> kt;
    for (const QJsonValue& k : keys.toArray()) kt.append(k.toDouble());
    time_slider_->set_keys(kt);
    auto_key_->blockSignals(true);
    auto_key_->setChecked(ask("autoKeyframe", QJsonObject{{"query", true}}).toBool());
    auto_key_->blockSignals(false);
    QString mode = st["mode"].toString();
    if (mode_buttons_.contains(mode)) mode_buttons_[mode]->setChecked(true);
    history_button_->setChecked(st["history"].toBool());
    update_title();
}

void MainWindow::update_title() {
    QString file = state()["file"].toString();
    setWindowTitle(QString("Aztek - %1").arg(file.isEmpty() ? "untitled" : file));
}

ViewportWidget* MainWindow::active_view() const {
    return panels_->active();
}

void MainWindow::set_tool(const QString& tool) {
    tool_ = tool;
    if (tool_buttons_.contains(tool)) tool_buttons_[tool]->setChecked(true);
    for (ViewportWidget* v : panels_->views()) v->set("tool", "\"" + tool + "\"");
    if (tool_settings_) tool_settings_->set_tool(tool);
    if (tool == "paintSkin")
        help_line_->setText("Paint Skin Weights Tool: drag to paint, Ctrl paints the inverse, Shift smooths, B-drag sizes the brush.");
    else if (tool == "joint")
        help_line_->setText("Joint Tool: click to place joints, each under the last. Enter ends the chain.");
    else
        help_line_->setText(QString("%1 Tool: drag a handle; middle-drag anywhere moves the active handle.").arg(tool.left(1).toUpper() + tool.mid(1)));
}

void MainWindow::set_mode(const QString& mode) {
    exec("selectMode", args(QJsonArray{mode}));
}

void MainWindow::repeat_last() {
    if (!last_command_.isEmpty()) exec(last_command_, last_call_);
}

void MainWindow::open_scene() {
    QString f = QFileDialog::getOpenFileName(this, "Open Scene", QString(), "Scenes (*.aztek *.ma);;Aztek scenes (*.aztek);;Maya ASCII (*.ma);;All files (*)");
    if (!f.isEmpty()) open_file(f);
}

void MainWindow::open_file(const QString& path) {
    QString err = take(aztek_file("open", path.toUtf8().constData()));
    Host::get()->poke();
    if (err.isEmpty()) {
        QSettings s;
        QStringList r = s.value("recentFiles").toStringList();
        r.removeAll(path);
        r.prepend(path);
        s.setValue("recentFiles", r.mid(0, 10));
        for (ViewportWidget* v : panels_->views()) v->set("frame", "\"all\"");
    }
    update_title();
}

void MainWindow::save_scene(bool as) {
    QString f = state()["file"].toString();
    if (as || f.isEmpty()) {
        QString filter;
        f = QFileDialog::getSaveFileName(this, "Save Scene As", f.isEmpty() ? "untitled.aztek" : f, "Aztek scenes (*.aztek);;Maya ASCII (*.ma)", &filter);
        if (f.isEmpty()) return;
        if (!f.section('/', -1).contains('.')) f += filter.contains("*.ma") ? ".ma" : ".aztek";
    }
    QString err = take(aztek_file("save", f.toUtf8().constData()));
    Host::get()->poke();
    update_title();
}

void MainWindow::show_script_editor() {
    if (!script_window_) {
        script_window_ = new QWidget(this, Qt::Window);
        script_window_->setWindowTitle("Script Editor");
        auto* l = new QVBoxLayout(script_window_);
        l->setContentsMargins(0, 0, 0, 0);
        script_editor_ = new ScriptEditor(script_window_);
        l->addWidget(script_editor_);
        script_window_->resize(760, 560);
        // Everything so far: the poller only hands over new lines, so ask the history again.
        script_editor_->append(QJsonArray{QJsonObject{{"text", "// Script Editor: MEL. Ctrl+Enter runs, keypad Enter runs and clears."}, {"kind", 3}}});
    }
    script_window_->show();
    script_window_->raise();
    script_window_->activateWindow();
}

void MainWindow::show_tool_settings() {
    tool_dock_->show();
    tool_dock_->raise();
}

void MainWindow::show_preferences() {
    auto* p = new Preferences(this);
    p->setAttribute(Qt::WA_DeleteOnClose);
    connect(p, &Preferences::changed, this, &MainWindow::apply_prefs);
    p->show();
}

void MainWindow::show_uv_editor() {
    if (!uv_window_) {
        uv_window_ = new UvEditor(this);
        uv_window_->setWindowFlag(Qt::Window);
        // Over the right of the main window, as Maya opens its editors over the panels.
        QRect g = geometry();
        uv_window_->move(g.x() + g.width() - uv_window_->width() - 340, g.y() + 140);
    }
    uv_window_->show();
    uv_window_->raise();
    static_cast<UvEditor*>(uv_window_)->refresh();
}

void MainWindow::show_graph_editor() {
    if (!graph_window_) {
        graph_window_ = new GraphEditor(this);
        graph_window_->setWindowFlag(Qt::Window);
        QRect g = geometry();
        graph_window_->move(g.x() + (g.width() - graph_window_->width()) / 2, g.y() + g.height() - graph_window_->height() - 140);
    }
    graph_window_->show();
    graph_window_->raise();
    static_cast<GraphEditor*>(graph_window_)->refresh();
}

void MainWindow::show_attribute_editor() {
    if (attr_dock_->isVisible() && !attr_dock_->visibleRegion().isEmpty()) {
        channel_dock_->raise();
        return;
    }
    attr_dock_->show();
    attr_dock_->raise();
}

// ---------------------------------------------------------------------------------------------------------------------
// Marking menus and hotkeys

void MainWindow::marking_menu(QPoint global, int mods) {
    ViewportWidget* v = active_view();
    QString mode = state()["mode"].toString();
    QList<MarkItem> items;
    auto run = [](const QString& c, const QJsonObject& call = QJsonObject()) { return [c, call] { exec(c, call); }; };
    if (mods & 1) {
        // Shift: the modelling tools for the current component type.
        if (mode == "face") {
            items = {{"Extrude Face", 0, run("polyExtrude")}, {"Bevel Face", 4, run("polyBevel")}, {"Bridge Face", 2, run("polyBridge")},
                     {"Inset Face", 6, run("polyInset")}, {"Poke Face", 1, run("polyPoke")}, {"Duplicate Face", 7, run("polyDuplicateFaces")},
                     {"Extract Face", 3, run("polyExtract")}, {"Delete Face", 5, run("deleteComponents")},
                     {"Triangulate", -1, run("polyTriangulate")}, {"Quadrangulate", -1, run("polyQuadrangulate")}, {"Flip", -1, run("polyFlip")}};
        } else if (mode == "edge") {
            items = {{"Extrude Edge", 0, run("polyExtrude")}, {"Bevel Edge", 4, run("polyBevel")}, {"Bridge", 2, run("polyBridge")},
                     {"Insert Edge Loop", 6, run("polyInsertLoop")}, {"Collapse Edge", 1, run("polyCollapse")}, {"Delete Edge", 5, run("polyDissolve")},
                     {"Offset Edge Loop", 7, run("polyOffsetLoop")}, {"Connect", 3, run("polyConnect")},
                     {"Spin Edge", -1, run("polySpinEdge")}, {"Fill Hole", -1, run("polyFillHole")}, {"Soften Edge", -1, run("polySoftEdge", {{"angle", 180.0}})},
                     {"Harden Edge", -1, run("polySoftEdge", {{"angle", 0.0}})}};
        } else if (mode == "vertex") {
            items = {{"Merge Vertices", 0, run("polyMerge")}, {"Merge to Center", 4, run("polyMergeCenter")}, {"Connect", 2, run("polyConnect")},
                     {"Delete Vertex", 6, run("polyDissolve")}, {"Target Weld", 1, run("polyTargetWeld")}};
        } else {
            items = {{"Combine", 0, run("polyCombine")}, {"Separate", 4, run("polySeparate")}, {"Smooth", 2, run("polySubdivide")},
                     {"Mirror", 6, run("polyMirror")}, {"Boolean Union", 1, run("polyBoolean", {{"operation", "union"}})},
                     {"Boolean Difference", 7, run("polyBoolean", {{"operation", "difference"}})},
                     {"Delete History", -1, run("delete", {{"constructionHistory", true}})}, {"Center Pivot", -1, run("centerPivot")},
                     {"Freeze Transformations", -1, run("freezeTransforms")}};
        }
    } else if (mods & 2) {
        // Ctrl: selection conversion.
        items = {{"To Edges", 0, run("convertSelection", args(QJsonArray{"edge"}))}, {"To Faces", 4, run("convertSelection", args(QJsonArray{"face"}))},
                 {"To Vertices", 6, run("convertSelection", args(QJsonArray{"vertex"}))}, {"Grow", 2, run("growSelection")},
                 {"Shrink", -1, run("shrinkSelection")}, {"Edge Loop", -1, run("selectLoop")}, {"Border", -1, run("selectBorder")}};
    } else {
        QString hit;
        if (v) {
            QPoint local = v->mapFromGlobal(global);
            double dpr = v->devicePixelRatioF();
            hit = take(aztek_view_hit(v->id(), local.x() * dpr, local.y() * dpr));
        }
        if (!hit.isEmpty()) {
            auto mode_to = [hit](const QString& m) {
                return [hit, m] {
                    // Hilite the object under the cursor, then switch.
                    QJsonArray sel = state()["selection"].toArray();
                    bool in = false;
                    for (const QJsonValue& s : sel)
                        if (s.toString().section('.', 0, 0) == hit) in = true;
                    if (!in) exec("select", QJsonObject{{"args", QJsonArray{hit}}});
                    exec("selectMode", QJsonObject{{"args", QJsonArray{m}}});
                };
            };
            items = {{"Edge", 0, mode_to("edge")}, {"Object Mode", 1, mode_to("object")}, {"UV", 2, {}, false}, {"Vertex Face", 3, {}, false},
                     {"Face", 4, mode_to("face")}, {"UV Shell", 5, {}, false}, {"Vertex", 6, mode_to("vertex")}, {"Multi", 7, {}, false},
                     {"Select", -1, run("select", args(QJsonArray{hit}))}, {"Select Hierarchy", -1, [hit] {
                          exec("select", QJsonObject{{"args", QJsonArray{hit}}});
                          exec("selectHierarchy");
                      }},
                     {"Attribute Editor...", -1, [this, hit] {
                          exec("select", QJsonObject{{"args", QJsonArray{hit}}});
                          show_attribute_editor();
                      }},
                     {"Delete", -1, run("delete", args(QJsonArray{hit}))}};
        } else {
            items = {{"Select All", -1, [] {
                          QJsonArray all;
                          for (const QJsonValue& n : query("outliner").toArray())
                              if (n.toObject()["type"].toString() != "objectSet") all.append(n.toObject()["path"].toString());
                          exec("select", args(all));
                      }},
                     {"Deselect", -1, run("select", {{"clear", true}})},
                     {"Undo", -1, [] { aztek_undo(); Host::get()->poke(); }},
                     {"Redo", -1, [] { aztek_redo(); Host::get()->poke(); }},
                     {"Repeat Last", -1, [this] { repeat_last(); }},
                     {"Frame All", -1, [v] { if (v) v->set("frame", "\"all\""); }},
                     {"Object Mode", 1, [this] { set_mode("object"); }}};
        }
    }
    MarkingMenu::show_at(global, items);
}

bool MainWindow::eventFilter(QObject* o, QEvent* e) {
    if (e->type() == QEvent::KeyPress || e->type() == QEvent::KeyRelease) {
        // Only for this window's widgets, and never while typing in a field.
        QWidget* w = qobject_cast<QWidget*>(o);
        if (!w || w->window() != this) return QMainWindow::eventFilter(o, e);
        if (typing_widget(QApplication::focusWidget())) return QMainWindow::eventFilter(o, e);
        if (o != QApplication::focusWidget() && QApplication::focusWidget()) return QMainWindow::eventFilter(o, e);
        auto* k = static_cast<QKeyEvent*>(e);
        if (hotkey(k, e->type() == QEvent::KeyPress)) return true;
    }
    return QMainWindow::eventFilter(o, e);
}

bool MainWindow::hotkey(QKeyEvent* k, bool press) {
    if (k->isAutoRepeat()) return k->key() == Qt::Key_Space;
    Qt::KeyboardModifiers m = k->modifiers() & ~Qt::KeypadModifier;
    bool ctrl = m & Qt::ControlModifier, shift = m & Qt::ShiftModifier, alt = m & Qt::AltModifier;
    int key = k->key();
    if (key == Qt::Key_Space) {
        if (press) {
            space_down_ms_ = QDateTime::currentMSecsSinceEpoch();
            space_held_ = true;
            QTimer::singleShot(220, this, [this] {
                if (space_held_ && hotbox_) hotbox_->popup(QCursor::pos());
            });
        } else {
            space_held_ = false;
            qint64 held = QDateTime::currentMSecsSinceEpoch() - space_down_ms_;
            QWidget* popup = QApplication::activePopupWidget();
            if (hotbox_ && hotbox_->isVisible() && !(popup && popup->inherits("QMenu"))) hotbox_->hide();
            if (held < 220) panels_->toggle();
        }
        return true;
    }
    // Held snapping: X to the grid, V to points, while dragging a move.
    if ((key == Qt::Key_X || key == Qt::Key_V) && !ctrl && !alt) {
        QString mode = press ? (key == Qt::Key_X ? "\"grid\"" : "\"point\"") : "\"none\"";
        for (ViewportWidget* vw : panels_->views()) vw->set("snap", mode);
        if (press) help_line_->setText(key == Qt::Key_X ? "Snap to grids" : "Snap to points");
        return true;
    }
    // D held, or Insert to toggle: the manipulator moves the pivot.
    if (key == Qt::Key_D && !ctrl && !alt && !shift) {
        for (ViewportWidget* vw : panels_->views()) vw->set("pivotMode", press ? "true" : "false");
        if (press) help_line_->setText("Pivot: drag the manipulator to move the pivot");
        return true;
    }
    if (key == Qt::Key_B && tool_ == "paintSkin" && !ctrl && !alt && !shift) {
        for (ViewportWidget* v : panels_->views()) v->set("brushAdjust", press ? "true" : "false");
        return true;
    }
    if (key == Qt::Key_T && ctrl && shift && press) {
        show_tool_settings();
        return true;
    }
    if (key == Qt::Key_Insert && press) {
        pivot_mode_ = !pivot_mode_;
        for (ViewportWidget* vw : panels_->views()) vw->set("pivotMode", pivot_mode_ ? "true" : "false");
        return true;
    }
    if (!press) return false;
    ViewportWidget* v = active_view();
    auto view_set = [v](const char* key, const QString& j) {
        if (v) v->set(key, j);
    };
    switch (key) {
    case Qt::Key_Q: if (!ctrl) { set_tool("select"); return true; } break;
    case Qt::Key_W:
        if (shift) exec("setKeyframe", QJsonObject{{"attribute", "translate"}});
        else set_tool("move");
        return true;
    case Qt::Key_E:
        if (ctrl) exec("polyExtrude", OptionsDialog::saved("polyExtrude"));
        else if (shift) exec("setKeyframe", QJsonObject{{"attribute", "rotate"}});
        else set_tool("rotate");
        return true;
    case Qt::Key_R:
        if (shift) exec("setKeyframe", QJsonObject{{"attribute", "scale"}});
        else set_tool("scale");
        return true;
    case Qt::Key_V: if (alt) { toggle_play(1); return true; } break;
    case Qt::Key_F2: menu_set_->setCurrentText("Animation"); return true;
    case Qt::Key_F3: menu_set_->setCurrentText("Modeling"); return true;
    case Qt::Key_F4: menu_set_->setCurrentText("Rigging"); return true;
    case Qt::Key_Return:
    case Qt::Key_Enter:
        // The joint tool's chain ends; the next click starts another.
        if (tool_ == "joint") { exec("select", QJsonObject{{"clear", true}}); return true; }
        break;
    case Qt::Key_Escape: if (play_timer_->isActive()) { stop_play(); return true; } break;
    case Qt::Key_F:
        if (shift)
            panels_->set_all("frame", "\"selection\"");
        else
            view_set("frame", "\"selection\"");
        return true;
    case Qt::Key_A:
        if (ctrl && shift) {
            QJsonArray all;
            for (const QJsonValue& n : query("outliner").toArray())
                if (n.toObject()["type"].toString() != "objectSet") all.append(n.toObject()["path"].toString());
            exec("select", args(all));
        } else if (ctrl)
            show_attribute_editor();
        else if (shift)
            panels_->set_all("frame", "\"all\"");
        else
            view_set("frame", "\"all\"");
        return true;
    case Qt::Key_Z:
        if (shift || (ctrl && shift)) aztek_redo();
        else aztek_undo();
        Host::get()->poke();
        return true;
    case Qt::Key_Y: if (ctrl) { aztek_redo(); Host::get()->poke(); return true; } break;
    case Qt::Key_G:
        if (ctrl) exec("group");
        else repeat_last();
        return true;
    case Qt::Key_D:
        if (ctrl && shift) OptionsDialog::open("duplicate", "Duplicate Special");
        else if (ctrl) exec("duplicate");
        else if (alt && shift) exec("delete", QJsonObject{{"constructionHistory", true}});
        else if (shift) exec("duplicateWithTransform");
        else return false;
        return true;
    case Qt::Key_P:
        if (shift) exec("parent", QJsonObject{{"world", true}});
        else exec("parent");
        return true;
    case Qt::Key_H:
        if (ctrl && shift) exec("showLastHidden");
        else if (ctrl) exec("hide");
        else exec("toggleVisibility");
        return true;
    case Qt::Key_B:
        if (ctrl) exec("polyBevel", OptionsDialog::saved("polyBevel"));
        else if (!alt && !shift) {
            bool on = ask("softSelect", QJsonObject{{"query", true}, {"queried", QJsonArray{"softSelectEnabled"}}}).toInt() != 0;
            exec("softSelect", QJsonObject{{"softSelectEnabled", on ? 0 : 1}});
            for (ViewportWidget* vw : panels_->views()) vw->update();
            help_line_->setText(on ? "Soft Select off" : "Soft Select on: B toggles; the falloff radius is in Modify > Soft Select options");
        } else return false;
        return true;
    case Qt::Key_N: if (ctrl) { aztek_free(aztek_file("new", "")); Host::get()->poke(); return true; } break;
    case Qt::Key_O: if (ctrl) { open_scene(); return true; } break;
    case Qt::Key_S:
        if (ctrl) save_scene(shift);
        else if (!alt) exec("setKeyframe");
        else return false;
        return true;
    case Qt::Key_I: if (ctrl && shift) { exec("invertSelection"); return true; } break;
    case Qt::Key_1: exec("smoothPreview", QJsonObject{{"level", 0}}); return true;
    case Qt::Key_2:
        for (ViewportWidget* vw : panels_->views()) vw->set("smoothCage", "true");
        exec("smoothPreview", QJsonObject{{"level", 2}});
        return true;
    case Qt::Key_3:
        for (ViewportWidget* vw : panels_->views()) vw->set("smoothCage", "false");
        exec("smoothPreview", QJsonObject{{"level", 2}});
        return true;
    case Qt::Key_4: view_set("wireframe", "true"); return true;
    case Qt::Key_5: view_set("wireframe", "false"); return true;
    case Qt::Key_F8: {
        QString mode = state()["mode"].toString();
        set_mode(mode == "object" ? "vertex" : "object");
        return true;
    }
    case Qt::Key_F9: if (ctrl) exec("convertSelection", args(QJsonArray{"vertex"})); else set_mode("vertex"); return true;
    case Qt::Key_F10: if (ctrl) exec("convertSelection", args(QJsonArray{"edge"})); else set_mode("edge"); return true;
    case Qt::Key_F11: if (ctrl) exec("convertSelection", args(QJsonArray{"face"})); else set_mode("face"); return true;
    case Qt::Key_Delete:
    case Qt::Key_Backspace:
        if (ctrl) exec("polyDissolve");
        else if (state()["mode"].toString() == "object") exec("delete");
        else exec("deleteComponents");
        return true;
    case Qt::Key_Greater:
    case Qt::Key_Period:
        if (shift || key == Qt::Key_Greater) exec("growSelection");
        else if (alt) time_slider_->set_time(time_slider_->time() + 1);
        else {
            QJsonValue f = ask("findKeyframe", QJsonObject{{"which", "next"}});
            if (f.isDouble()) time_slider_->set_time(f.toDouble());
        }
        return true;
    case Qt::Key_Less:
    case Qt::Key_Comma:
        if (shift || key == Qt::Key_Less) exec("shrinkSelection");
        else if (alt) time_slider_->set_time(time_slider_->time() - 1);
        else {
            QJsonValue f = ask("findKeyframe", QJsonObject{{"which", "previous"}});
            if (f.isDouble()) time_slider_->set_time(f.toDouble());
        }
        return true;
    case Qt::Key_Plus:
    case Qt::Key_Equal:
    case Qt::Key_Minus: {
        QSettings s;
        double m2 = s.value("prefs/manipSize", 1.0).toDouble();
        m2 = key == Qt::Key_Minus ? qMax(0.25, m2 - 0.25) : qMin(4.0, m2 + 0.25);
        s.setValue("prefs/manipSize", m2);
        apply_prefs();
        return true;
    }
    default: break;
    }
    return false;
}

void MainWindow::closeEvent(QCloseEvent* e) {
    QSettings s;
    s.setValue("window/geometry", saveGeometry());
    s.setValue("window/state", saveState());
    e->accept();
}

void MainWindow::run_startup(const QString& script, const QString& screenshot) {
    QSettings s;
    if (s.contains("window/geometry") && screenshot.isEmpty()) {
        restoreGeometry(s.value("window/geometry").toByteArray());
        restoreState(s.value("window/state").toByteArray());
    }
    exec_quiet("constructionHistory", QJsonObject{{"state", s.value("prefs/constructionHistory", true).toBool()}});
    if (!script.isEmpty()) {
        QTimer::singleShot(300, this, [this, script] {
            QFile f(script);
            if (f.open(QIODevice::ReadOnly)) {
                QString src = QString::fromUtf8(f.readAll());
                if (script.endsWith(".py"))
                    python(src);
                else
                    mel(src);
            }
            for (ViewportWidget* v : panels_->views()) v->set("frame", "\"all\"");
        });
    }
    if (!screenshot.isEmpty()) {
        QTimer::singleShot(1500, this, [this, screenshot] {
            // The whole window: grab the Qt widgets, then paint each viewport's own pixels over its spot.
            QImage img = grab().toImage();
            QPainter p(&img);
            for (ViewportWidget* v : panels_->views()) {
                if (!v->isVisible()) continue;
                QString tmp = screenshot + ".view.ppm";
                if (!v->capture(tmp)) continue;
                QImage vi(tmp);
                QFile::remove(tmp);
                QPoint at = v->mapTo(this, QPoint(0, 0));
                p.drawImage(QRect(at, v->size()), vi);
            }
            p.end();
            img.save(screenshot);
            qApp->quit();
        });
    }
}

} // namespace az
