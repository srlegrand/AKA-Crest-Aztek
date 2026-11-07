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
    bool intermediates_ = false;
    bool updating_ = false;
    QSet<QString> expanded_;
    QLineEdit* filter_;
};

// The channels picked in the channel box, by attribute name (translateX...).
QStringList selected_channels();
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
    explicit AttributeEditor(QWidget* parent = nullptr);
protected:
    void refresh() override;
private:
    QTabWidget* tabs_;
    QString current_tab_;
};

class ScriptEditor : public QWidget {
    Q_OBJECT
public:
    explicit ScriptEditor(QWidget* parent = nullptr);
    void append(const QJsonArray& lines);
    void run(bool clear_after);
protected:
    bool eventFilter(QObject* o, QEvent* e) override;
private:
    QPlainTextEdit* history_;
    QTabWidget* inputs_;
};

} // namespace az
