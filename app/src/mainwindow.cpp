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
#include "dopesheet.h"
#include "nodeeditor.h"
#include "shelves.h"
#include "editors.h"

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
#include <QJsonDocument>
#include <QKeyEvent>
#include <QLabel>
#include <QMouseEvent>
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

    build_runtime();
    load_hotkeys();
    build_status_line();
    addToolBarBreak();
    QToolBar* shelf_holder = addToolBar("Shelf");
    shelf_holder->setObjectName("shelf");
    shelf_holder->setMovable(false);
    shelf_bar_ = new ShelfBar([this](const QString& code, const QString& lang) {
        if (lang == "python") python(code);
        else if (lang == "menu") run_menu_path(code);
        else mel(code);
    }, shelf_holder);
    shelf_capture() = [this](QAction* a) { add_to_shelf(a); };
    shelf_holder->addWidget(shelf_bar_);
    build_toolbox();
    build_docks();
    build_bottom();
    build_menus();

    // The centre: model panels.
    setCentralWidget(panels_);
    in_view_ = new InViewEditor(panels_);

    connect(Host::get(), &Host::sceneChanged, this, &MainWindow::refresh_state);
    connect(Host::get(), &Host::logLines, this, [this](const QJsonArray& lines) {
        if (script_editor_) script_editor_->append(lines);
        // The command line's result field shows the latest result or error.
        for (const QJsonValue& v : lines) {
            QJsonObject l = v.toObject();
            int k = l["kind"].toInt();
            if (k == 1 || k == 2 || k == 4) command_line_->show_result(l["text"].toString(), k == 2);
            if (k == 0) {
                remember_command(l["text"].toString());
                // After an edit that made a history node: its attributes in the view.
                QString t = l["text"].toString();
                bool off = QSettings().value("prefs/inViewEditorOff", false).toBool();
                if (!off && t.startsWith("poly") && !t.startsWith("polyEvaluate")) QTimer::singleShot(0, this, [this] { in_view_->show_last(); });
                else if (t.startsWith("select")) in_view_->hide();
            }
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
    // What Ctrl+Shift+click puts on the shelf.
    QJsonObject echo{{"name", command}, {"call", call}};
    a->setProperty("melCommand", query("melEcho", QString::fromUtf8(QJsonDocument(echo).toJson(QJsonDocument::Compact))).toString());
    a->setStatusTip(ok ? QString("%1: %2").arg(label, command) : QString("%1: not available yet").arg(label));
    return a;
}

QAction* MainWindow::todo(QMenu* m, const QString& label) {
    QAction* a = m->addAction(label);
    a->setEnabled(false);
    a->setStatusTip(label + ": not available yet");
    return a;
}

QMenu* MainWindow::create_menu() {
    auto* m = new QMenu("Create", this);
    QMenu* polys = m->addMenu("Polygon Primitives");
    struct P { const char* label; const char* cmd; };
    for (P p : {P{"Sphere", "polySphere"}, P{"Cube", "polyCube"}, P{"Cylinder", "polyCylinder"}, P{"Cone", "polyCone"}, P{"Torus", "polyTorus"},
                P{"Plane", "polyPlane"}, P{"Disc", "polyDisc"}, P{"Pipe", "polyPipe"}, P{"Capsule", "polyCapsule"}, P{"Platonic Solid", "polyIcoSphere"},
                P{"Quad Sphere", "polyQuadSphere"}})
        cmd(polys, p.label, p.cmd, {}, QString(), true);
    QMenu* nurbs = m->addMenu("NURBS Primitives");
    const QJsonArray up{0, 1, 0};
    cmd(nurbs, "Sphere", "sphere", QJsonObject{{"axis", up}}, QString(), true);
    cmd(nurbs, "Cube", "nurbsCube", QJsonObject{{"axis", up}}, QString(), true);
    cmd(nurbs, "Cylinder", "cylinder", QJsonObject{{"axis", up}}, QString(), true);
    cmd(nurbs, "Cone", "cone", QJsonObject{{"axis", up}}, QString(), true);
    cmd(nurbs, "Plane", "nurbsPlane", QJsonObject{{"axis", up}}, QString(), true);
    cmd(nurbs, "Torus", "torus", QJsonObject{{"axis", up}}, QString(), true);
    nurbs->addSeparator();
    cmd(nurbs, "Circle", "circle", QJsonObject{{"normal", up}}, QString(), true);
    cmd(nurbs, "Square", "nurbsSquare", {}, QString(), true);
    QMenu* curves = m->addMenu("Curve Tools");
    curves->addAction("CV Curve Tool", [this] { set_tool("curveCv"); });
    curves->addAction("EP Curve Tool", [this] { set_tool("curveEp"); });
    m->addSeparator();
    cmd(m, "Locator", "spaceLocator");
    cmd(m, "Empty Group", "group", QJsonObject{{"empty", true}}, "Ctrl+G");
    cmd(m, "Sets", "createSet", {}, QString(), true);
    return m;
}

QMenu* MainWindow::mesh_menu() {
    auto* m = new QMenu("Mesh", this);
    QMenu* b = m->addMenu("Booleans");
    cmd(b, "Union", "polyBoolean", QJsonObject{{"operation", "union"}});
    cmd(b, "Difference", "polyBoolean", QJsonObject{{"operation", "difference"}});
    cmd(b, "Intersection", "polyBoolean", QJsonObject{{"operation", "intersection"}});
    cmd(m, "Combine", "polyUnite", {}, QString(), true);
    cmd(m, "Separate", "polySeparate");
    m->addSeparator();
    cmd(m, "Conform", "polyConformToObject", {}, QString(), true);
    cmd(m, "Fill Hole", "polyFillHole", {}, QString(), true);
    cmd(m, "Reduce", "polyReduce", {}, QString(), true);
    cmd(m, "Remesh", "polyRemesh", {}, QString(), true);
    cmd(m, "Retopologize", "polyRetopo", {}, QString(), true);
    cmd(m, "Smooth", "polySmooth", {}, QString(), true);
    cmd(m, "Average Vertices", "polyAverageVertex", {}, QString(), true);
    cmd(m, "Triangulate", "polyTriangulate", {}, QString(), true);
    cmd(m, "Quadrangulate", "polyQuadrangulate", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Mirror", "polyMirrorFace", {}, QString(), true);
    cmd(m, "Symmetrize", "symmetrize", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Transfer Attributes", "transferAttributes", {}, QString(), true);
    cmd(m, "Cleanup...", "polyCleanup", QJsonObject{{"selectOnly", true}, {"nsided", true}, {"nonManifold", true}, {"lamina", true}, {"zeroGeometry", true}}, QString(), true);
    cmd(m, "Mesh Check", "meshCheck", {}, QString(), true);
    return m;
}

QMenu* MainWindow::edit_mesh_menu() {
    auto* m = new QMenu("Edit Mesh", this);
    QLabel* comp = new QLabel("  Components", m);
    comp->setObjectName("menuSection");
    cmd(m, "Bevel", "polyBevel", {}, "Ctrl+B", true);
    cmd(m, "Add Divisions", "polySubdivideFacet", {}, QString(), true);
    cmd(m, "Bridge", "polyBridge", {}, QString(), true);
    cmd(m, "Chamfer Vertices", "polyChamferVtx", {}, QString(), true);
    cmd(m, "Collapse", "polyCollapse");
    cmd(m, "Connect", "polyConnectComponents", {}, QString(), true);
    cmd(m, "Detach", "polyDetachEdges");
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
    cmd(m, "Flip Triangle Edge", "polyFlipEdge");
    cmd(m, "Spin Edge", "polySpinEdge");
    cmd(m, "Poke Face", "polyPoke", {}, QString(), true);
    cmd(m, "Wedge", "polyWedgeFace", {}, QString(), true);
    cmd(m, "Assign Invisible Faces", "polyHole", {}, QString(), true);
    cmd(m, "Duplicate", "polyChipOff", {}, QString(), true);
    cmd(m, "Extract", "polyExtract", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Grid Fill", "polyGridFill", {}, QString(), true);
    cmd(m, "Split Rings", "polySplitRings", {}, QString(), true);
    return m;
}

QMenu* MainWindow::mesh_tools_menu() {
    auto* m = new QMenu("Mesh Tools", this);
    auto tool = [this, m](const QString& label, const QString& id) { m->addAction(label, [this, id] { set_tool(id); }); };
    tool("Append to Polygon", "appendPolygon");
    cmd(m, "Connect", "polyConnect");
    tool("Crease", "crease");
    tool("Create Polygon", "createPolygon");
    tool("Insert Edge Loop", "insertLoop");
    tool("Multi-Cut", "multiCut");
    tool("Offset Edge Loop", "offsetLoop");
    tool("Quad Draw", "quadDraw");
    tool("Slide Edge", "slideEdge");
    tool("Target Weld", "targetWeld");
    QMenu* sculpt = m->addMenu("Sculpting Tools");
    for (const char* b : {"sculpt", "smooth", "relax", "grab", "pinch", "flatten", "foamy", "spray", "repeat", "imprint", "wax", "scrape", "fill", "knife", "smear", "bulge", "amplify", "freeze"}) {
        QString brush = b;
        QString label = brush.left(1).toUpper() + brush.mid(1) + " Tool";
        sculpt->addAction(label, [this, brush] {
            exec_quiet("sculptMeshCacheCtx", QJsonObject{{"edit", true}, {"sculptTool", brush}});
            set_tool("sculpt");
        });
    }
    m->addSeparator();
    m->addAction("Make Live", [] { exec("makeLive"); });
    m->addAction("Make Not Live", [] { exec("makeLive", QJsonObject{{"none", true}}); });
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
    cmd(m, "Average Normals", "polyAverageNormal", {}, QString(), true);
    cmd(m, "Set to Face", "polySetToFaceNormal");
    cmd(m, "Lock Normals", "polyLockNormals");
    cmd(m, "Unlock Normals", "polyUnlockNormals");
    cmd(m, "Face Weighted Normals", "polyWeightedNormals", {}, QString(), true);
    return m;
}

QMenu* MainWindow::curves_menu() {
    auto* m = new QMenu("Curves", this);
    cmd(m, "Attach", "attachCurve", {}, QString(), true);
    cmd(m, "Detach", "detachCurve", {}, QString(), true);
    cmd(m, "Open/Close", "closeCurve", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Insert Knot", "insertKnotCurve", {}, QString(), true);
    cmd(m, "Extend", "extendCurve", {}, QString(), true);
    cmd(m, "Offset", "offsetCurve", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Reverse Direction", "reverseCurve");
    cmd(m, "Rebuild", "rebuildCurve", {}, QString(), true);
    cmd(m, "Fit B-spline", "fitBspline");
    cmd(m, "Smooth", "smoothCurve", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Polygon Edges to Curve", "polyToCurve", {}, QString(), true);
    return m;
}

QMenu* MainWindow::surfaces_menu() {
    auto* m = new QMenu("Surfaces", this);
    cmd(m, "Loft", "loft", {}, QString(), true);
    cmd(m, "Planar", "planar");
    cmd(m, "Revolve", "revolve", {}, QString(), true);
    cmd(m, "Birail", "singleProfileBirailSurface", {}, QString(), true);
    cmd(m, "Extrude", "extrude", {}, QString(), true);
    cmd(m, "Boundary", "boundary", {}, QString(), true);
    cmd(m, "Square", "square", {}, QString(), true);
    cmd(m, "Sweep Mesh", "sweepMeshFromCurve", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Duplicate NURBS Patch Curve", "duplicateCurve", {}, QString(), true);
    cmd(m, "Attach", "attachSurface", {}, QString(), true);
    cmd(m, "Detach", "detachSurface", {}, QString(), true);
    cmd(m, "Open/Close", "closeSurface", {}, QString(), true);
    cmd(m, "Insert Isoparms", "insertKnotSurface", {}, QString(), true);
    cmd(m, "Offset", "offsetSurface", {}, QString(), true);
    cmd(m, "Rebuild", "rebuildSurface", {}, QString(), true);
    cmd(m, "Reverse Direction", "reverseSurface", {}, QString(), true);
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
    cmd(m, "Lattice", "lattice", QJsonObject{{"divisions", QJsonArray{2, 5, 2}}, {"objectCentered", true}}, QString(), true);
    cmd(m, "Wrap", "wrap", {}, QString(), true);
    cmd(m, "Wire", "wire", {}, QString(), true);
    cmd(m, "Soft Modification", "softMod", {}, QString(), true);
    cmd(m, "Delta Mush", "deltaMush", {}, QString(), true);
    cmd(m, "Tension", "tension", {}, QString(), true);
    return m;
}

QMenu* MainWindow::skeleton_menu() {
    auto* m = new QMenu("Skeleton", this);
    m->addAction("Create Joints", [this] { set_tool("joint"); });
    // Runs on the selected joint until there is a tool to drag the new joint with.
    m->addAction("Insert Joint Tool", [this] { set_tool("insertJoint"); });
    cmd(m, "Mirror Skeleton", "mirrorJoint", QJsonObject{{"mirrorXY", true}, {"mirrorBehavior", true}}, QString(), true);
    cmd(m, "Mirror Skeleton (Orientation)", "mirrorJoint", QJsonObject{{"mirrorXY", true}});
    m->addAction("Orient Joint", [] { exec("joint", QJsonObject{{"edit", true}, {"orientJoint", "xyz"}, {"secondaryAxisOrient", "yup"}, {"children", true}}); });
    m->addSeparator();
    cmd(m, "Remove Joint", "removeJoint");
    cmd(m, "Connect Joint", "connectJoint", QJsonObject{{"connectMode", true}});
    cmd(m, "Parent Joint", "connectJoint", QJsonObject{{"parentMode", true}});
    cmd(m, "Disconnect Joint", "disconnectJoint");
    cmd(m, "Reroot Skeleton", "reroot");
    m->addSeparator();
    cmd(m, "Set Preferred Angle", "joint", QJsonObject{{"edit", true}, {"setPreferredAngles", true}, {"children", true}});
    cmd(m, "Assume Preferred Angle", "joint", QJsonObject{{"edit", true}, {"assumePreferredAngles", true}, {"children", true}});
    m->addSeparator();
    QMenu* hik = m->addMenu("HumanIK");
    cmd(hik, "Create Skeleton", "hikCreateSkeleton", QJsonObject{{"character", true}}, QString(), true);
    cmd(hik, "Define Character from Selection", "hikCreateCharacter");
    hik->addAction("Create Control Rig...", [] {
        exec("hikCreateControlRig", QJsonObject{{"args", QJsonArray{"Character1"}}});
    });
    m->addSeparator();
    cmd(m, "Create IK Handle", "ikHandle", QJsonObject{{"solver", "ikRPsolver"}}, QString(), true);
    cmd(m, "Create IK Handle (Single Chain)", "ikHandle", QJsonObject{{"solver", "ikSCsolver"}});
    cmd(m, "Create IK Spline Handle", "ikHandle", QJsonObject{{"solver", "ikSplineSolver"}}, QString(), true);
    cmd(m, "Enable IK Handles", "ikHandle", QJsonObject{{"enableHandles", true}});
    cmd(m, "Disable IK Handles", "ikHandle", QJsonObject{{"disableHandles", true}});
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
    cmd(m, "Pole Vector", "poleVectorConstraint", {}, QString(), true);
    cmd(m, "Geometry", "geometryConstraint", {}, QString(), true);
    cmd(m, "Normal", "normalConstraint", {}, QString(), true);
    cmd(m, "Tangent", "tangentConstraint", {}, QString(), true);
    cmd(m, "Point on Poly", "pointOnPolyConstraint", {}, QString(), true);
    m->addSeparator();
    cmd(m, "Remove Target", "RemoveConstraintTarget");
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
    cmd(m, "Set Breakdown", "setKeyframe", QJsonObject{{"breakdown", true}}, QString(), false);
    m->addAction("Convert to Breakdown", [this] { exec("keyframe", QJsonObject{{"edit", true}, {"time", time_slider_->time()}, {"breakdown", true}}); });
    m->addAction("Convert to Key", [this] { exec("keyframe", QJsonObject{{"edit", true}, {"time", time_slider_->time()}, {"breakdown", false}}); });
    m->addSeparator();
    m->addAction("Copy Keys", [this] { exec("copyKey", QJsonObject{{"time", QString("%1").arg(time_slider_->time())}}); });
    m->addAction("Paste Keys", [this] { exec("pasteKey", QJsonObject{{"time", time_slider_->time()}, {"option", "merge"}}); });
    cmd(m, "Bake Animation", "bakeResults", {}, QString(), true);
    m->addSeparator();
    auto* sdk = m->addMenu("Set Driven Key");
    sdk->addAction("Set...", this, &MainWindow::show_set_driven_key);
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
    auto* path = m->addMenu("Motion Paths");
    cmd(path, "Attach to Motion Path", "pathAnimation", QJsonObject{{"follow", true}, {"fractionMode", true}, {"startTimeU", 1}, {"endTimeU", 120}});
    cmd(path, "Flow Path Object", "flow");
    auto* layers = m->addMenu("Animation Layers");
    cmd(layers, "Create Layer", "animLayer");
    cmd(layers, "Create Layer from Selected", "animLayer", QJsonObject{{"addSelectedObjects", true}});
    cmd(layers, "Merge Layers", "mergeAnimLayers");
    auto* vis = m->addMenu("Visualize");
    cmd(vis, "Create Motion Trail", "snapshot", QJsonObject{{"motionTrail", true}});
    cmd(vis, "Animation Snapshot", "snapshot", QJsonObject{{"constructionHistory", true}});
    cmd(vis, "Ghost Selected", "GhostSelected");
    cmd(vis, "Unghost Selected", "UnghostSelected");
    cmd(vis, "Unghost All", "UnghostAll");
    cmd(m, "Create Character Set", "character");
    m->addSeparator();
    m->addAction("Graph Editor...", [this] { show_graph_editor(); });
    m->addAction("Dope Sheet...", [this] { show_dope_sheet(); });
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
        menus << mesh_menu() << edit_mesh_menu() << mesh_tools_menu() << mesh_display_menu() << curves_menu() << surfaces_menu() << uv_menu() << deform_menu();
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
    hotbox_ = new Hotbox(set_menus_, 7, this);
    hotbox_->set_zones([this](int zone) { return hotbox_zone(zone); });
    connect(hotbox_, &Hotbox::spaceReleased, this, [this] {
        held_.remove(Qt::Key_Space);
        space_held_ = false;
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// Status line, shelf, toolbox

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
    // Double-clicking a tool opens its settings, as in Maya.
    for (const QString& id : tool_buttons_.keys())
        if (QWidget* w = tb->widgetForAction(tool_buttons_[id])) {
            w->setProperty("toolId", id);
            w->installEventFilter(this);
        }
    tb->addSeparator();
    tb->addAction(icon("single"), "Single Perspective View", [this] { panels_->set_layout(0); });
    tb->addAction(icon("four"), "Four View", [this] { panels_->set_layout(4); });
    tb->addAction(icon("perspOutliner"), "Perspective/Outliner", [this] {
        panels_->set_layout(0);
        outliner_dock_->show();
    });
    QAction* last = tb->addAction(icon("lastTool"), "Last Tool (Y)", [this] { set_tool(last_tool_); });
    Q_UNUSED(last);
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
    connect(tool_settings_, &ToolSettings::axisOrientation, this, [this](const QString& m) {
        for (ViewportWidget* v : panels_->views()) v->set("axisOrientation", "\"" + m + "\"");
    });
    connect(tool_settings_, &ToolSettings::stepSnap, this, [this](const QString& k, double x) {
        for (ViewportWidget* v : panels_->views()) v->set(k, QString::number(x));
    });
    connect(tool_settings_, &ToolSettings::curveDegree, this, [this](int d) {
        for (ViewportWidget* v : panels_->views()) v->set("curveDegree", QString::number(d));
    });
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
    connect(time_slider_, &TimeSlider::contextMenuAt, this, &MainWindow::time_slider_menu);
    connect(time_slider_, &TimeSlider::rangeMoved, this, [](double from, double to, double by) {
        exec("keyframe", QJsonObject{{"edit", true}, {"time", QString("%1:%2").arg(from).arg(to)}, {"relative", true}, {"timeChange", by}});
    });
    row1->addWidget(time_slider_, 1);
    row1->addWidget(current);
    auto* play = new QToolBar(w);
    play->setIconSize(QSize(16, 16));
    play_timer_ = new QTimer(this);
    play_timer_->setInterval(1000 / 24);
    connect(play_timer_, &QTimer::timeout, this, [this] {
        // Real time goes by the clock, dropping frames to keep up; every frame goes one at a time.
        double t;
        if (play_every_frame_) {
            t = time_slider_->time() + play_dir_;
        } else {
            double fr = ask("playbackOptions", QJsonObject{{"query", true}, {"queried", QJsonArray{"framesPerSecond"}}}).toDouble(24);
            t = play_started_frame_ + play_dir_ * (QDateTime::currentMSecsSinceEpoch() - play_started_ms_) / 1000.0 * fr * play_speed_;
        }
        double a = time_slider_->start(), b = time_slider_->end();
        if (t > b || t < a) {
            if (loop_mode_ == "Once") {
                time_slider_->set_time(t > b ? b : a);
                stop_play();
                return;
            }
            if (loop_mode_ == "Oscillate") play_dir_ = -play_dir_;
            t = loop_mode_ == "Oscillate" ? (t > b ? b : a) : (t > b ? a : b);
            play_started_frame_ = t;
            play_started_ms_ = QDateTime::currentMSecsSinceEpoch();
        }
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
    connect(range_slider_, &RangeSlider::toggleFullRange, this, [this] {
        double mn = play_start_->value(), mx = play_end_->value();
        bool full = mn == anim_start_->value() && mx == anim_end_->value();
        if (full && last_full_max_ > last_full_min_) {
            exec_quiet("playbackOptions", QJsonObject{{"minTime", last_full_min_}, {"maxTime", last_full_max_}});
        } else {
            last_full_min_ = mn;
            last_full_max_ = mx;
            exec_quiet("playbackOptions", QJsonObject{{"minTime", anim_start_->value()}, {"maxTime", anim_end_->value()}});
        }
        sync_time_range();
    });
    row2->addWidget(anim_start_);
    row2->addWidget(play_start_);
    row2->addWidget(range_slider_, 1);
    row2->addWidget(play_end_);
    row2->addWidget(anim_end_);
    // Time units by name, as the Settings have them.
    auto* fps = new QComboBox(w);
    fps_box_ = fps;
    fps->setToolTip("Time unit");
    struct U { const char* label; double rate; };
    for (U u : {U{"Film (24 fps)", 24}, U{"PAL (25 fps)", 25}, U{"NTSC (30 fps)", 30}, U{"Show (48 fps)", 48}, U{"PAL Field (50 fps)", 50}, U{"NTSC Field (60 fps)", 60}}) fps->addItem(u.label, u.rate);
    connect(fps, &QComboBox::activated, this, [fps](int i) {
        exec("playbackOptions", QJsonObject{{"framesPerSecond", fps->itemData(i).toDouble()}});
    });
    row2->addWidget(fps);
    auto* speed = new QComboBox(w);
    speed_box_ = speed;
    speed->setToolTip("Playback speed");
    speed->addItems({"Play every frame", "Real-time", "Half", "Twice"});
    speed->setCurrentText(QSettings().value("prefs/playbackSpeed", "Real-time").toString());
    connect(speed, &QComboBox::activated, this, [this, speed](int) {
        QSettings().setValue("prefs/playbackSpeed", speed->currentText());
        apply_prefs();
    });
    row2->addWidget(speed);
    auto* loop = new QComboBox(w);
    loop->setToolTip("Looping");
    loop->addItems({"Once", "Oscillate", "Continuous"});
    loop->setCurrentText(QSettings().value("prefs/playbackLoop", "Continuous").toString());
    connect(loop, &QComboBox::activated, this, [this, loop](int) {
        QSettings().setValue("prefs/playbackLoop", loop->currentText());
        loop_mode_ = loop->currentText();
    });
    row2->addWidget(loop);
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


// ---------------------------------------------------------------------------------------------------------------------
// State

void MainWindow::toggle_play(int dir) {
    if (play_timer_->isActive() && play_dir_ == dir) {
        stop_play();
        return;
    }
    play_dir_ = dir;
    ask("play", QJsonObject{{"state", true}, {"forward", dir > 0}});
    double fps = ask("playbackOptions", QJsonObject{{"query", true}, {"queried", QJsonArray{"framesPerSecond"}}}).toDouble(24);
    play_timer_->setInterval(int(1000 / qMax(1.0, fps * play_speed_)));
    play_started_ms_ = QDateTime::currentMSecsSinceEpoch();
    play_started_frame_ = time_slider_->time();
    play_timer_->start();
}

void MainWindow::stop_play() {
    play_timer_->stop();
    ask("play", QJsonObject{{"state", false}});
}

void MainWindow::sync_time_range() {
    if (fps_box_) {
        double fr = ask("playbackOptions", QJsonObject{{"query", true}, {"queried", QJsonArray{"framesPerSecond"}}}).toDouble(24);
        int i = fps_box_->findData(fr);
        if (i >= 0) fps_box_->setCurrentIndex(i);
    }
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
        handle_ui_request(n);
    }
    // A script may have started or stopped playback with the play command.
    bool script_plays = ask("play", QJsonObject{{"query", true}, {"queried", QJsonArray{"state"}}}).toBool();
    if (script_plays && !play_timer_->isActive())
        toggle_play(ask("play", QJsonObject{{"query", true}, {"queried", QJsonArray{"forward"}}}).toBool(true) ? 1 : -1);
    else if (!script_plays && play_timer_->isActive())
        play_timer_->stop();
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
    QVector<double> bd;
    if (!kt.isEmpty())
        for (const QJsonValue& k : ask("keyframe", QJsonObject{{"query", true}, {"queried", QJsonArray{"breakdown"}}}).toArray()) bd.append(k.toDouble());
    time_slider_->set_breakdowns(bd);
    auto_key_->blockSignals(true);
    auto_key_->setChecked(ask("autoKeyframe", QJsonObject{{"query", true}}).toBool());
    auto_key_->blockSignals(false);
    QString mode = st["mode"].toString();
    if (mode_buttons_.contains(mode)) mode_buttons_[mode]->setChecked(true);
    history_button_->setChecked(st["history"].toBool());
    sync_status_line();
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
    if (tool != tool_) last_tool_ = tool_;
    tool_ = tool;
    if (tool_buttons_.contains(tool)) tool_buttons_[tool]->setChecked(true);
    for (ViewportWidget* v : panels_->views()) v->set("tool", "\"" + tool + "\"");
    if (tool_settings_) tool_settings_->set_tool(tool);
    static const QHash<QString, QString> tool_help{
        {"lasso", "Lasso Tool: drag round what to select. Shift toggles, Ctrl deselects, Ctrl+Shift adds."},
        {"paintSelect", "Paint Selection Tool: drag over components to select them; Ctrl deselects."},
        {"shortestPath", "Shortest Edge Path Tool: click a component, then another: the path between them is selected."},
        {"camTumble", "Tumble Tool: drag to tumble the view."},
        {"camTrack", "Track Tool: drag to track the view."},
        {"camDolly", "Dolly Tool: drag to dolly the view."},
        {"camZoom", "Zoom Tool: drag to change the lens."},
        {"camRoll", "Roll Tool: drag to roll the view."},
        {"camYawPitch", "Yaw-Pitch Tool: drag to turn the camera where it stands."},
        {"sculpt", "Sculpting: drag to sculpt, Ctrl inverts, Shift smooths, B-drag sizes the brush."},
        {"insertJoint", "Insert Joint: drag from a joint to add a joint along the bone."},
        {"multiCut", "Multi-Cut: click to place cut points, Shift snaps along edges, Ctrl-click inserts a loop, Backspace takes the last back, Enter cuts."},
        {"insertLoop", "Insert Edge Loop: hover an edge to see the loop, click to insert it."},
        {"offsetLoop", "Offset Edge Loop: drag from an edge to add loops either side."},
        {"targetWeld", "Target Weld: drag from a vertex or edge onto another."},
        {"slideEdge", "Slide Edge: pick edges, then middle-drag to slide them."},
        {"crease", "Crease: pick edges, then middle-drag to set their crease."},
        {"createPolygon", "Create Polygon: click to place points, Enter makes the face."},
        {"appendPolygon", "Append to Polygon: click border edges and points, Enter makes the face."},
        {"quadDraw", "Quad Draw: click places dots, Shift-click fills a quad, Ctrl-click inserts a loop, Ctrl+Shift-click deletes, Tab-drag extends a border edge, drag moves."},
    };
    if (tool_help.contains(tool))
        help_line_->setText(tool_help[tool]);
    else if (tool == "curveCv" || tool == "curveEp")
        help_line_->setText("Curve Tool: click to place points, Backspace takes the last back, Enter makes the curve. Hold X to snap to the grid.");
    else if (tool == "paintSkin")
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

void MainWindow::show_node_editor() {
    if (!node_window_) {
        node_window_ = new QWidget(this, Qt::Window);
        node_window_->setWindowTitle("Node Editor");
        auto* l = new QVBoxLayout(node_window_);
        l->setContentsMargins(0, 0, 0, 0);
        l->setSpacing(0);
        auto* bar = new QToolBar(node_window_);
        auto* ed = new NodeEditor(node_window_);
        bar->addAction("Input and Output Connections", [ed] { ed->graph_selection(1); });
        bar->addAction("Input Connections Deep", [ed] { ed->graph_selection(4); });
        bar->addAction("Add Selected", [ed] { ed->add_selected(); });
        bar->addAction("Clear", [ed] { ed->clear_graph(); });
        l->addWidget(bar);
        l->addWidget(ed, 1);
        node_window_->resize(1000, 640);
    }
    node_window_->show();
    node_window_->raise();
    node_window_->activateWindow();
    if (auto* ed = node_window_->findChild<NodeEditor*>()) ed->graph_selection(1);
}

void MainWindow::show_dope_sheet() {
    if (!dope_window_) {
        dope_window_ = new QWidget(this, Qt::Window);
        dope_window_->setWindowTitle("Dope Sheet");
        auto* l = new QVBoxLayout(dope_window_);
        l->setContentsMargins(0, 0, 0, 0);
        l->addWidget(new DopeSheet(dope_window_));
        dope_window_->resize(1000, 460);
    }
    dope_window_->show();
    dope_window_->raise();
    dope_window_->activateWindow();
}

void MainWindow::show_set_driven_key() {
    if (!sdk_window_) sdk_window_ = new SetDrivenKeyWindow(this);
    sdk_window_->show();
    sdk_window_->raise();
    sdk_window_->activateWindow();
}

void MainWindow::time_slider_menu(QPoint global) {
    QMenu m;
    QString range = time_slider_->has_range() ? time_slider_->range_text() : QString::number(time_slider_->time());
    auto keyed = [range](const QString& command, QJsonObject call = QJsonObject()) {
        call["time"] = range;
        return [command, call] { exec(command, call); };
    };
    m.addAction("Cut", keyed("cutKey"));
    m.addAction("Copy", keyed("copyKey"));
    m.addAction("Paste", [this] { exec("pasteKey", QJsonObject{{"time", time_slider_->time()}, {"option", "merge"}}); });
    m.addAction("Delete", keyed("cutKey", QJsonObject{{"clear", true}}));
    m.addSeparator();
    m.addAction("Snap", keyed("snapKey"));
    m.addAction("Convert to Breakdown", keyed("keyframe", QJsonObject{{"edit", true}, {"breakdown", true}}));
    m.addAction("Convert to Key", keyed("keyframe", QJsonObject{{"edit", true}, {"breakdown", false}}));
    m.addSeparator();
    QMenu* tan = m.addMenu("Tangents");
    for (const char* t : {"auto", "spline", "clamped", "linear", "flat", "step", "plateau"}) {
        QString label = QString(t);
        label[0] = label[0].toUpper();
        tan->addAction(label, keyed("keyTangent", QJsonObject{{"inTangentType", t == QString("step") ? "clamped" : t}, {"outTangentType", t}}));
    }
    m.exec(global);
}

void MainWindow::show_tool_settings() {
    tool_dock_->show();
    tool_dock_->raise();
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

bool MainWindow::eventFilter(QObject* o, QEvent* e) {
    // Ctrl+Shift+click on a menu item puts it on the shelf.
    if (e->type() == QEvent::MouseButtonRelease) {
        if (auto* menu = qobject_cast<QMenu*>(o)) {
            auto* me = static_cast<QMouseEvent*>(e);
            if ((me->modifiers() & (Qt::ControlModifier | Qt::ShiftModifier)) == (Qt::ControlModifier | Qt::ShiftModifier)) {
                if (QAction* a = menu->actionAt(me->position().toPoint())) {
                    add_to_shelf(a);
                    while (QWidget* p = QApplication::activePopupWidget()) p->close();
                    return true;
                }
            }
        }
    }
    // A tool key held with a left press in a view: the tool's marking menu. K and I work a drag.
    if (!view_hold_.isEmpty() && qobject_cast<ViewportWidget*>(o) && (e->type() == QEvent::MouseButtonPress || e->type() == QEvent::MouseMove || e->type() == QEvent::MouseButtonRelease)) {
        auto* me = static_cast<QMouseEvent*>(e);
        if (view_hold_ == "scrub" || view_hold_ == "nudgeKeys") {
            view_scrub(o, e);
            return true;
        }
        if (e->type() == QEvent::MouseButtonPress && me->button() == Qt::LeftButton && !(me->modifiers() & Qt::AltModifier)) {
            tool_marking_menu(me->globalPosition().toPoint(), view_hold_);
            return true;
        }
    }
    if (e->type() == QEvent::MouseButtonDblClick && o->property("toolId").isValid()) {
        set_tool(o->property("toolId").toString());
        show_tool_settings();
        return true;
    }
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
