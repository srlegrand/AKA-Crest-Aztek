// The main window: menus by menu set, status line, shelf, toolbox, model panels, editors, time slider, command line
// and the hotkeys.
#pragma once
#include <QHash>
#include <QJsonObject>
#include <QMainWindow>
#include <functional>

class QActionGroup;
class QComboBox;
class QDockWidget;
class QLabel;
class QMenu;
class QTabWidget;
class QToolBar;
class QToolButton;
class QSpinBox;
class QTimer;

namespace az {

class PanelLayout;
class ScriptEditor;
class CommandLine;
class TimeSlider;
class RangeSlider;
class Hotbox;
class ViewportWidget;
class ToolSettings;

class MainWindow : public QMainWindow {
    Q_OBJECT
public:
    MainWindow();
    PanelLayout* panels() const { return panels_; }
    void run_startup(const QString& script, const QString& screenshot);
    void open_file(const QString& path);
protected:
    bool eventFilter(QObject* o, QEvent* e) override;
    void closeEvent(QCloseEvent* e) override;
private:
    // Menus.
    void build_menus();
    void build_menu_set(const QString& set);
    QMenu* file_menu();
    QMenu* edit_menu();
    QMenu* create_menu();
    QMenu* select_menu();
    QMenu* modify_menu();
    QMenu* display_menu();
    QMenu* windows_menu();
    QMenu* mesh_menu();
    QMenu* edit_mesh_menu();
    QMenu* mesh_tools_menu();
    QMenu* mesh_display_menu();
    QMenu* curves_menu();
    QMenu* surfaces_menu();
    QMenu* uv_menu();
    QMenu* deform_menu();
    QMenu* skeleton_menu();
    QMenu* skin_menu();
    QMenu* constrain_menu();
    QMenu* control_menu();
    QMenu* key_menu();
    QMenu* playback_menu();
    QAction* cmd(QMenu* m, const QString& label, const QString& command, const QJsonObject& call = QJsonObject(), const QString& shortcut = QString(), bool options = false);
    QAction* todo(QMenu* m, const QString& label);

    // Bars and panels.
    void build_status_line();
    void build_shelf();
    void build_toolbox();
    void build_docks();
    void build_bottom();
    void apply_prefs();

    // State.
    void refresh_state();
    void set_tool(const QString& tool);
    void set_mode(const QString& mode);
    ViewportWidget* active_view() const;
    void marking_menu(QPoint global, int mods);
    void repeat_last();
    void open_scene();
    void save_scene(bool as);
    bool hotkey(QKeyEvent* k, bool press);
    void show_script_editor();
    void show_attribute_editor();
    void show_graph_editor();
    void show_uv_editor();
    void show_preferences();
    void show_tool_settings();
    void show_dope_sheet();
    void show_node_editor();
    void show_set_driven_key();
    void time_slider_menu(QPoint global);
    void update_title();
    void toggle_play(int dir);
    void stop_play();
    void sync_time_range();

    PanelLayout* panels_ = nullptr;
    ScriptEditor* script_editor_ = nullptr;
    QWidget* script_window_ = nullptr;
    QWidget* graph_window_ = nullptr;
    QWidget* uv_window_ = nullptr;
    QWidget* dope_window_ = nullptr;
    QWidget* node_window_ = nullptr;
    QWidget* sdk_window_ = nullptr;
    CommandLine* command_line_ = nullptr;
    TimeSlider* time_slider_ = nullptr;
    RangeSlider* range_slider_ = nullptr;
    QSpinBox *anim_start_ = nullptr, *play_start_ = nullptr, *play_end_ = nullptr, *anim_end_ = nullptr, *current_frame_ = nullptr;
    QTimer* play_timer_ = nullptr;
    int play_dir_ = 1;
    QAction* auto_key_ = nullptr;
    QLabel* help_line_ = nullptr;
    ToolSettings* tool_settings_ = nullptr;
    QDockWidget* tool_dock_ = nullptr;
    QComboBox* menu_set_ = nullptr;
    QList<QMenu*> set_menus_;
    QHash<QString, QAction*> mode_buttons_;
    QHash<QString, QAction*> tool_buttons_;
    QAction* history_button_ = nullptr;
    QDockWidget* outliner_dock_ = nullptr;
    QDockWidget* channel_dock_ = nullptr;
    QDockWidget* attr_dock_ = nullptr;
    Hotbox* hotbox_ = nullptr;
    QString tool_ = "select";
    QString last_command_;
    QJsonObject last_call_;
    qint64 space_down_ms_ = 0;
    bool space_held_ = false;
    bool pivot_mode_ = false;
};

} // namespace az
