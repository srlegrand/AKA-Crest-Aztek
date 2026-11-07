// The docked editors: outliner, channel box with the layer editor, attribute editor, script editor.
#pragma once
#include <QJsonObject>
#include <QSet>
#include <QWidget>

class QTreeWidget;
class QTreeWidgetItem;
class QPlainTextEdit;
class QTabWidget;
class QVBoxLayout;
class QScrollArea;
class QLineEdit;
class QLabel;
class QToolButton;

namespace az {

// Runs refresh() once after the scene settles rather than on every change.
class Panel : public QWidget {
    Q_OBJECT
public:
    explicit Panel(QWidget* parent = nullptr);
protected:
    virtual void refresh() = 0;
    void schedule();
    void showEvent(QShowEvent*) override;
private:
    bool pending_ = false;
};

class Outliner : public Panel {
    Q_OBJECT
public:
    explicit Outliner(QWidget* parent = nullptr);
protected:
    void refresh() override;
private:
    void add(QTreeWidgetItem* parent, const QJsonObject& node);
    void selection_changed();
    QTreeWidget* tree_;
    bool shapes_ = false;
    bool updating_ = false;
    QSet<QString> expanded_;
    QLineEdit* filter_;
    bool dag_only_ = false;
    bool attrs_ = false;
    bool hide_cameras_ = false;
    bool show_icons_ = true;
    QSet<QString> hidden_types_;
};

// The channels picked in the channel box, by attribute name (translateX...).
QStringList selected_channels();
// The picked channels that are a history node's, as whole plugs: they don't follow the selected object.
QStringList selected_input_plugs();
void clear_channel_selection();

class ChannelBox : public Panel {
    Q_OBJECT
public:
    explicit ChannelBox(QWidget* parent = nullptr);
protected:
    void refresh() override;
private:
    void add_channels(QVBoxLayout* lay, const QJsonArray& channels);
    QScrollArea* scroll_;
    QWidget* body_ = nullptr;
    QSet<QString> open_inputs_;
};

class LayerEditor : public Panel {
    Q_OBJECT
public:
    explicit LayerEditor(QWidget* parent = nullptr);
protected:
    void refresh() override;
private:
    QTreeWidget* list_;
};

class AttributeEditor : public Panel {
    Q_OBJECT
public:
    // pinned: stays on this node (Copy Tab); empty follows the selection.
    explicit AttributeEditor(QWidget* parent = nullptr, const QString& pinned = QString());
protected:
    void refresh() override;
    bool eventFilter(QObject* o, QEvent* e) override;
private:
    QTabWidget* tabs_;
    QString current_tab_;
    QString pinned_;
};

class ScriptEditor : public QWidget {
    Q_OBJECT
public:
    explicit ScriptEditor(QWidget* parent = nullptr);
    ~ScriptEditor() override;
    void append(const QJsonArray& lines);
    void run(bool clear_after);
protected:
    bool eventFilter(QObject* o, QEvent* e) override;
private:
    void add_tab(const QString& language, const QString& name, const QString& text, const QString& path = QString());
    void rename_tab(int i);
    void close_tab(int i);
    void save_current(bool as);
    void store_tabs();
    QPlainTextEdit* current() const;
    QPlainTextEdit* history_;
    QTabWidget* inputs_;
    bool show_numbers_ = true;
    bool stack_trace_ = true;
    bool echo_all_ = false;
};

} // namespace az
