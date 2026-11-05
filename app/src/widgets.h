// Smaller pieces of the main window: option boxes, the time and range sliders, the command line, the hotbox and
// the preferences window.
#pragma once
#include <QDialog>
#include <QJsonObject>
#include <QLineEdit>
#include <QMenu>
#include <QWidget>
#include <QWidgetAction>
#include <functional>

class QToolButton;
class QLabel;
class QDoubleSpinBox;
class QSpinBox;

namespace az {

// A command's settings, built from its flags and remembered between sessions.
class OptionsDialog : public QDialog {
    Q_OBJECT
public:
    static void open(const QString& command, const QString& title);
    // The remembered settings of a command, as a call.
    static QJsonObject saved(const QString& command);
private:
    OptionsDialog(const QString& command, const QString& title);
    QJsonObject collect() const;
    void store(const QJsonObject& call) const;
    QString command_;
    QList<std::function<void(QJsonObject&)>> readers_;
};

// A menu item with Maya's option box on its right.
class OptionRow : public QWidgetAction {
    Q_OBJECT
public:
    OptionRow(const QString& label, const QString& shortcut, std::function<void()> run, std::function<void()> options, QObject* parent);
protected:
    QWidget* createWidget(QWidget* parent) override;
private:
    QString label_, shortcut_;
    std::function<void()> run_, options_;
};

class TimeSlider : public QWidget {
    Q_OBJECT
public:
    explicit TimeSlider(QWidget* parent = nullptr);
    double time() const { return time_; }
    void set_time(double t);
    void set_range(double a, double b);
    double start() const { return start_; }
    double end() const { return end_; }
    void set_keys(const QVector<double>& k);
    const QVector<double>& keys() const { return keys_; }
signals:
    void timeChanged(double t);
protected:
    void paintEvent(QPaintEvent*) override;
    void mousePressEvent(QMouseEvent*) override;
    void mouseMoveEvent(QMouseEvent*) override;
private:
    double frame_at(int x) const;
    double time_ = 1, start_ = 1, end_ = 120;
    QVector<double> keys_;
};

// The bar under the time slider: the playback range inside the animation range, dragged by its ends or middle.
class RangeSlider : public QWidget {
    Q_OBJECT
public:
    explicit RangeSlider(QWidget* parent = nullptr);
    void set(double ast, double min, double max, double aet);
signals:
    void rangeChanged(double min, double max);
protected:
    void paintEvent(QPaintEvent*) override;
    void mousePressEvent(QMouseEvent*) override;
    void mouseMoveEvent(QMouseEvent*) override;
    void mouseReleaseEvent(QMouseEvent*) override;
private:
    double frame_at(double x) const;
    double x_of(double f) const;
    double ast_ = 1, min_ = 1, max_ = 120, aet_ = 200;
    int grab_ = -1;
    double grab_at_ = 0, grab_min_ = 0, grab_max_ = 0;
};

class CommandLine : public QWidget {
    Q_OBJECT
public:
    explicit CommandLine(QWidget* parent = nullptr);
    void show_result(const QString& text, bool error);
signals:
    void openScriptEditor();
protected:
    bool eventFilter(QObject* o, QEvent* e) override;
private:
    QToolButton* lang_;
    QLineEdit* input_;
    QLineEdit* result_;
    QStringList history_;
    int pos_ = 0;
};

// Space held: every menu at the cursor.
class Hotbox : public QWidget {
    Q_OBJECT
public:
    Hotbox(const QList<QMenu*>& menus, QWidget* parent);
    void popup(QPoint global);
protected:
    void paintEvent(QPaintEvent*) override;
};

class Preferences : public QDialog {
    Q_OBJECT
public:
    explicit Preferences(QWidget* parent);
signals:
    void changed();
};

} // namespace az
