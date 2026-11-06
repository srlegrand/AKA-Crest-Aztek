#include "widgets.h"
#include "host.h"
#include "icons.h"

#include <QApplication>
#include <QCheckBox>
#include <QColorDialog>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QImage>
#include <QLabel>
#include <QListWidget>
#include <QMouseEvent>
#include <QPainter>
#include <QPushButton>
#include <QSettings>
#include <QSpinBox>
#include <QStackedWidget>
#include <QToolButton>
#include <QVBoxLayout>
#include <cmath>

#ifndef AZTEK_VERSION
#define AZTEK_VERSION "dev"
#endif

namespace az {

QPixmap splash_pixmap() {
    QPixmap pm(640, 340);
    pm.fill(QColor(0, 0, 0));
    QPainter p(&pm);
    p.setRenderHint(QPainter::SmoothPixmapTransform);
    QImage logo(":/aztek-logo.png");
    QRect r(70, 20, 500, 253);
    p.drawImage(r, logo);
    p.setPen(QColor(150, 150, 150));
    QFont f = p.font();
    f.setPointSize(9);
    p.setFont(f);
    p.drawText(QRect(0, 280, 620, 20), Qt::AlignRight, QString("Version %1").arg(AZTEK_VERSION));
    return pm;
}

void show_about(QWidget* parent) {
    QDialog d(parent);
    d.setWindowTitle("About Aztek");
    auto* v = new QVBoxLayout(&d);
    auto* img = new QLabel(&d);
    img->setPixmap(splash_pixmap().scaledToWidth(480, Qt::SmoothTransformation));
    v->addWidget(img);
    auto* text = new QLabel(QString("Aztek %1<br>Modelling, UVs, rigging and animation.<br>Rust core, Qt interface, Vulkan viewport.<br><br>"
                                    "<a href=\"https://github.com/srlegrand/AKA-Crest-Aztek\" style=\"color:#7fb4d6\">github.com/srlegrand/AKA-Crest-Aztek</a>")
                                .arg(AZTEK_VERSION),
                            &d);
    text->setOpenExternalLinks(true);
    text->setTextFormat(Qt::RichText);
    v->addWidget(text);
    auto* ok = new QDialogButtonBox(QDialogButtonBox::Ok, &d);
    QObject::connect(ok, &QDialogButtonBox::accepted, &d, &QDialog::accept);
    v->addWidget(ok);
    d.exec();
}


// ---------------------------------------------------------------------------------------------------------------------
// Option boxes

namespace {

QJsonObject command_def(const QString& name) {
    static QJsonArray all;
    if (all.isEmpty()) all = query("commands").toArray();
    for (const QJsonValue& c : all)
        if (c.toObject()["name"].toString() == name) return c.toObject();
    return QJsonObject();
}

// The flag definitions carry no types in the commands list, so read them off the defaults the command reports.
QJsonValue flag_default(const QString& cmd, const QString& flag) {
    static QHash<QString, QJsonObject> cache;
    if (!cache.contains(cmd)) cache[cmd] = query("flagDefaults", cmd).toObject();
    return cache[cmd].value(flag);
}

QString nice(const QString& name) {
    QString out;
    for (int i = 0; i < name.size(); ++i) {
        QChar c = name[i];
        if (i == 0)
            out += c.toUpper();
        else if (c.isUpper()) {
            out += ' ';
            out += c;
        } else
            out += c;
    }
    return out;
}

} // namespace

QJsonObject OptionsDialog::saved(const QString& command) {
    QSettings s;
    QJsonObject call;
    s.beginGroup("options/" + command);
    for (const QString& k : s.childKeys()) {
        QJsonValue v = parse(s.value(k).toString());
        if (!v.isUndefined() && !v.isNull()) call[k] = v;
    }
    return call;
}

void OptionsDialog::open(const QString& command, const QString& title) {
    auto* d = new OptionsDialog(command, title);
    d->setAttribute(Qt::WA_DeleteOnClose);
    d->show();
}

OptionsDialog::OptionsDialog(const QString& command, const QString& title) : QDialog(QApplication::activeWindow()), command_(command) {
    setWindowTitle(title + " Options");
    QJsonObject def = command_def(command);
    QJsonObject saved_call = saved(command);
    auto* lay = new QVBoxLayout(this);
    auto* doc = new QLabel(def["doc"].toString(), this);
    doc->setWordWrap(true);
    doc->setObjectName("optionsDoc");
    lay->addWidget(doc);
    auto* form = new QFormLayout();
    form->setLabelAlignment(Qt::AlignRight);
    for (const QJsonValue& fv : def["flags"].toArray()) {
        QJsonObject f = fv.toObject();
        QString name = f["name"].toString();
        if (name == "args" || name == "constructionHistory" || name == "name") continue;
        QJsonValue dflt = saved_call.contains(name) ? saved_call[name] : flag_default(command, name);
        QWidget* w = nullptr;
        if (dflt.isBool()) {
            auto* cb = new QCheckBox(this);
            cb->setChecked(dflt.toBool());
            readers_.append([cb, name](QJsonObject& o) { o[name] = cb->isChecked(); });
            w = cb;
        } else if (dflt.isDouble()) {
            double x = dflt.toDouble();
            bool integral = x == std::floor(x) && std::abs(x) < 1e9 && !flag_default(command, name).toString().contains('.');
            QJsonValue raw = flag_default(command, name);
            // Integers stay integers when the default was one.
            if (raw.isDouble() && raw.toDouble() == std::floor(raw.toDouble()) && f["doc"].toString().contains("count", Qt::CaseInsensitive)) integral = true;
            auto* sb = new QDoubleSpinBox(this);
            sb->setRange(-1e9, 1e9);
            sb->setDecimals(integral && raw.toDouble() == std::floor(raw.toDouble()) ? 0 : 4);
            sb->setValue(x);
            bool as_int = sb->decimals() == 0;
            readers_.append([sb, name, as_int](QJsonObject& o) { o[name] = as_int ? QJsonValue(qint64(sb->value())) : QJsonValue(sb->value()); });
            w = sb;
        } else if (dflt.isString()) {
            auto* e = new QLineEdit(dflt.toString(), this);
            readers_.append([e, name](QJsonObject& o) {
                if (!e->text().isEmpty()) o[name] = e->text();
            });
            w = e;
        } else if (dflt.isArray() && dflt.toArray().size() == 3) {
            auto* row = new QWidget(this);
            auto* h = new QHBoxLayout(row);
            h->setContentsMargins(0, 0, 0, 0);
            QList<QDoubleSpinBox*> sbs;
            for (int k = 0; k < 3; ++k) {
                auto* sb = new QDoubleSpinBox(row);
                sb->setRange(-1e9, 1e9);
                sb->setDecimals(4);
                sb->setValue(dflt.toArray()[k].toDouble());
                h->addWidget(sb);
                sbs << sb;
            }
            readers_.append([sbs, name](QJsonObject& o) { o[name] = QJsonArray{sbs[0]->value(), sbs[1]->value(), sbs[2]->value()}; });
            w = row;
        } else {
            continue;
        }
        w->setToolTip(f["doc"].toString());
        auto* label = new QLabel(nice(name) + ":", this);
        label->setToolTip(f["doc"].toString());
        form->addRow(label, w);
    }
    lay->addLayout(form);
    lay->addStretch(1);
    auto* buttons = new QHBoxLayout();
    auto* apply_close = new QPushButton(title, this);
    auto* apply = new QPushButton("Apply", this);
    auto* close_button = new QPushButton("Close", this);
    auto* reset = new QPushButton("Reset Settings", this);
    buttons->addWidget(apply_close);
    buttons->addWidget(apply);
    buttons->addWidget(close_button);
    lay->addWidget(reset, 0, Qt::AlignLeft);
    lay->addLayout(buttons);
    auto run = [this] {
        QJsonObject call = collect();
        store(call);
        az::exec(command_, call);
    };
    connect(apply_close, &QPushButton::clicked, this, [this, run] {
        run();
        close();
    });
    connect(apply, &QPushButton::clicked, this, run);
    connect(close_button, &QPushButton::clicked, this, &QDialog::close);
    connect(reset, &QPushButton::clicked, this, [this, title] {
        QSettings s;
        s.remove("options/" + command_);
        QString cmd = command_;
        close();
        OptionsDialog::open(cmd, title);
    });
    resize(440, sizeHint().height());
}

QJsonObject OptionsDialog::collect() const {
    QJsonObject o;
    for (auto& r : readers_) r(o);
    return o;
}

void OptionsDialog::store(const QJsonObject& call) const {
    QSettings s;
    s.beginGroup("options/" + command_);
    for (auto it = call.begin(); it != call.end(); ++it) {
        QJsonArray wrap{it.value()};
        QString text = QString::fromUtf8(QJsonDocument(wrap).toJson(QJsonDocument::Compact));
        s.setValue(it.key(), text.mid(1, text.size() - 2));
    }
}

// ---------------------------------------------------------------------------------------------------------------------

namespace {

class RowWidget : public QWidget {
public:
    RowWidget(const QString& label, const QString& shortcut, std::function<void()> run, std::function<void()> options, QWidget* parent)
        : QWidget(parent), run_(std::move(run)), options_(std::move(options)) {
        setMouseTracking(true);
        setAttribute(Qt::WA_Hover);
        auto* h = new QHBoxLayout(this);
        h->setContentsMargins(26, 2, 4, 2);
        h->setSpacing(6);
        auto* l = new QLabel(label, this);
        h->addWidget(l, 1);
        if (!shortcut.isEmpty()) {
            auto* s = new QLabel(shortcut, this);
            s->setObjectName("menuShortcut");
            h->addWidget(s);
        }
        box_ = new QLabel(this);
        box_->setPixmap(icon("options", 14).pixmap(14, 14));
        box_->setFixedSize(16, 16);
        box_->setToolTip("Options");
        h->addWidget(box_);
        setMinimumHeight(22);
    }
protected:
    void mouseReleaseEvent(QMouseEvent* e) override {
        bool box = box_->geometry().adjusted(-4, -4, 4, 4).contains(e->position().toPoint());
        // Close every open menu before running, as a normal item would.
        while (QWidget* p = QApplication::activePopupWidget()) p->close();
        if (box)
            options_();
        else
            run_();
    }
    void paintEvent(QPaintEvent*) override {
        if (underMouse()) {
            QPainter p(this);
            p.fillRect(rect(), QColor(82, 133, 166));
        }
    }
    void enterEvent(QEnterEvent*) override { update(); }
    void leaveEvent(QEvent*) override { update(); }
private:
    std::function<void()> run_, options_;
    QLabel* box_;
};

} // namespace

OptionRow::OptionRow(const QString& label, const QString& shortcut, std::function<void()> run, std::function<void()> options, QObject* parent)
    : QWidgetAction(parent), label_(label), shortcut_(shortcut), run_(std::move(run)), options_(std::move(options)) {
    setText(label);
    connect(this, &QAction::triggered, this, [this] { run_(); });
}

QWidget* OptionRow::createWidget(QWidget* parent) {
    return new RowWidget(label_, shortcut_, run_, options_, parent);
}

// ---------------------------------------------------------------------------------------------------------------------
// Time slider

TimeSlider::TimeSlider(QWidget* parent) : QWidget(parent) {
    setMinimumHeight(30);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    setObjectName("timeSlider");
}

void TimeSlider::set_time(double t) {
    t = qBound(start_, std::round(t), end_);
    if (t == time_) return;
    time_ = t;
    update();
    emit timeChanged(t);
}

void TimeSlider::set_range(double a, double b) {
    start_ = a;
    end_ = qMax(a + 1, b);
    time_ = qBound(start_, time_, end_);
    update();
}

void TimeSlider::set_keys(const QVector<double>& k) {
    if (k == keys_) return;
    keys_ = k;
    update();
}

void TimeSlider::set_breakdowns(const QVector<double>& k) {
    if (k == breakdowns_) return;
    breakdowns_ = k;
    update();
}

QString TimeSlider::range_text() const {
    double a = qMin(range_a_, range_b_), b = qMax(range_a_, range_b_);
    return QString("%1:%2").arg(a).arg(b);
}

double TimeSlider::frame_at(int x) const {
    double w = qMax(1, width() - 16);
    return start_ + (x - 8) / w * (end_ - start_);
}

void TimeSlider::paintEvent(QPaintEvent*) {
    QPainter p(this);
    p.fillRect(rect(), QColor(45, 45, 45));
    double w = width() - 16;
    double span = end_ - start_;
    // A tick per frame when there's room, labels every few.
    int step = 1;
    for (int s : {1, 2, 5, 10, 20, 50, 100, 200, 500, 1000})
        if (w / (span / s) >= 40) {
            step = s;
            break;
        }
    p.setPen(QColor(150, 150, 150));
    QFont f = font();
    f.setPointSizeF(qMax(6.5, f.pointSizeF() - 1));
    p.setFont(f);
    for (int fr = int(std::ceil(start_)); fr <= int(end_); ++fr) {
        double x = 8 + (fr - start_) / span * w;
        bool major = fr % step == 0;
        if (w / span >= 4 || major) p.drawLine(QPointF(x, height() - (major ? 10 : 5)), QPointF(x, height()));
        if (major) p.drawText(QPointF(x + 2, height() - 12), QString::number(fr));
    }
    double fw = qMax(2.0, w / span);
    // The range selected with Shift-drag, red as in Maya.
    if (range_a_ != range_b_) {
        double a = 8 + (qMin(range_a_, range_b_) + move_by_ - start_) / span * w;
        double b = 8 + (qMax(range_a_, range_b_) + move_by_ - start_) / span * w;
        p.fillRect(QRectF(a, 0, b - a, height()), QColor(200, 60, 60, 110));
    }
    // Keys on the selection: red bars; breakdowns green.
    for (double k : keys_) {
        if (k < start_ || k > end_) continue;
        double kx = 8 + (k - start_) / span * w;
        p.fillRect(QRectF(kx - qMax(1.0, fw * 0.25), 3, qMax(2.0, fw * 0.5), height() - 6), breakdowns_.contains(k) ? QColor(60, 190, 80) : QColor(204, 40, 40));
    }
    double x = 8 + (time_ - start_) / span * w;
    p.fillRect(QRectF(x - fw / 2, 0, fw, height()), QColor(210, 210, 210, 120));
    p.setPen(QColor(230, 230, 230));
    p.drawText(QRectF(x + fw / 2 + 2, 0, 60, 14), Qt::AlignLeft | Qt::AlignTop, QString::number(time_));
}

void TimeSlider::mousePressEvent(QMouseEvent* e) {
    double fr = std::round(frame_at(int(e->position().x())));
    if (e->button() == Qt::RightButton) {
        emit contextMenuAt(e->globalPosition().toPoint());
        return;
    }
    if (e->modifiers() & Qt::ShiftModifier) {
        selecting_ = true;
        range_a_ = fr;
        range_b_ = fr + 1;
        update();
        return;
    }
    double lo = qMin(range_a_, range_b_), hi = qMax(range_a_, range_b_);
    if (range_a_ != range_b_ && fr >= lo && fr <= hi) {
        moving_ = true;
        move_from_ = fr;
        move_by_ = 0;
        return;
    }
    range_a_ = range_b_ = 0;
    set_time(fr);
}

void TimeSlider::mouseMoveEvent(QMouseEvent* e) {
    if (!e->buttons()) return;
    double fr = std::round(frame_at(int(e->position().x())));
    if (selecting_) {
        range_b_ = fr >= range_a_ ? fr + 1 : fr;
        update();
    } else if (moving_) {
        move_by_ = fr - move_from_;
        update();
    } else {
        set_time(fr);
    }
}

void TimeSlider::mouseReleaseEvent(QMouseEvent*) {
    if (moving_ && move_by_ != 0) {
        double lo = qMin(range_a_, range_b_), hi = qMax(range_a_, range_b_);
        emit rangeMoved(lo, hi, move_by_);
        range_a_ += move_by_;
        range_b_ += move_by_;
    }
    moving_ = false;
    move_by_ = 0;
    selecting_ = false;
    double lo = qMin(range_a_, range_b_), hi = qMax(range_a_, range_b_);
    ask("timeControl", QJsonObject{{"edit", true}, {"rangeArray", lo != hi ? QJsonArray{lo, hi} : QJsonArray{}}});
    update();
}

// ---------------------------------------------------------------------------------------------------------------------
// Range slider

RangeSlider::RangeSlider(QWidget* parent) : QWidget(parent) {
    setMinimumHeight(18);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
}

void RangeSlider::set(double ast, double min, double max, double aet) {
    if (ast == ast_ && min == min_ && max == max_ && aet == aet_) return;
    ast_ = ast, min_ = min, max_ = max, aet_ = qMax(ast + 1, aet);
    update();
}

double RangeSlider::x_of(double f) const {
    return 4 + (f - ast_) / (aet_ - ast_) * (width() - 8);
}

double RangeSlider::frame_at(double x) const {
    return ast_ + (x - 4) / qMax(1, width() - 8) * (aet_ - ast_);
}

void RangeSlider::paintEvent(QPaintEvent*) {
    QPainter p(this);
    p.fillRect(rect(), QColor(43, 43, 43));
    QRectF bar(x_of(min_), 2, qMax(8.0, x_of(max_) - x_of(min_)), height() - 4);
    p.setRenderHint(QPainter::Antialiasing);
    p.setPen(QColor(40, 40, 40));
    p.setBrush(QColor(120, 120, 120));
    p.drawRoundedRect(bar, 3, 3);
    p.setPen(QColor(230, 230, 230));
    QFont f = font();
    f.setPointSizeF(qMax(6.5, f.pointSizeF() - 1));
    p.setFont(f);
    p.drawText(bar.adjusted(4, 0, -4, 0), Qt::AlignVCenter | Qt::AlignLeft, QString::number(min_));
    p.drawText(bar.adjusted(4, 0, -4, 0), Qt::AlignVCenter | Qt::AlignRight, QString::number(max_));
}

void RangeSlider::mousePressEvent(QMouseEvent* e) {
    double x = e->position().x();
    double a = x_of(min_), b = x_of(max_);
    grab_ = std::abs(x - a) < 8 ? 0 : std::abs(x - b) < 8 ? 1 : (x > a && x < b ? 2 : -1);
    grab_at_ = frame_at(x);
    grab_min_ = min_, grab_max_ = max_;
}

void RangeSlider::mouseMoveEvent(QMouseEvent* e) {
    if (grab_ < 0) return;
    double d = std::round(frame_at(e->position().x()) - grab_at_);
    double mn = min_, mx = max_;
    if (grab_ == 0) mn = qMin(grab_min_ + d, max_ - 1);
    else if (grab_ == 1) mx = qMax(grab_max_ + d, min_ + 1);
    else {
        d = qBound(ast_ - grab_min_, d, aet_ - grab_max_);
        mn = grab_min_ + d, mx = grab_max_ + d;
    }
    mn = qMax(ast_, mn), mx = qMin(aet_, mx);
    if (mn != min_ || mx != max_) {
        min_ = mn, max_ = mx;
        update();
        emit rangeChanged(min_, max_);
    }
}

void RangeSlider::mouseReleaseEvent(QMouseEvent*) {
    grab_ = -1;
}

// ---------------------------------------------------------------------------------------------------------------------
// Command line

CommandLine::CommandLine(QWidget* parent) : QWidget(parent) {
    auto* h = new QHBoxLayout(this);
    h->setContentsMargins(2, 1, 2, 1);
    h->setSpacing(2);
    lang_ = new QToolButton(this);
    lang_->setText("MEL");
    lang_->setObjectName("cmdLang");
    lang_->setToolTip("Command language: click to switch between MEL and Python");
    connect(lang_, &QToolButton::clicked, this, [this] { lang_->setText(lang_->text() == "MEL" ? "Python" : "MEL"); });
    input_ = new QLineEdit(this);
    input_->setObjectName("cmdInput");
    input_->installEventFilter(this);
    result_ = new QLineEdit(this);
    result_->setReadOnly(true);
    result_->setObjectName("cmdResult");
    auto* se = new QToolButton(this);
    se->setIcon(icon("script", 18));
    se->setToolTip("Script Editor");
    connect(se, &QToolButton::clicked, this, &CommandLine::openScriptEditor);
    h->addWidget(lang_);
    h->addWidget(input_, 3);
    h->addWidget(result_, 2);
    h->addWidget(se);
    connect(input_, &QLineEdit::returnPressed, this, [this] {
        QString src = input_->text().trimmed();
        if (src.isEmpty()) return;
        history_.removeAll(src);
        history_.append(src);
        pos_ = history_.size();
        input_->clear();
        if (lang_->text() == "Python")
            python(src);
        else
            mel(src);
    });
}

void CommandLine::show_result(const QString& text, bool error) {
    result_->setText(text);
    result_->setProperty("error", error);
    result_->style()->unpolish(result_);
    result_->style()->polish(result_);
}

bool CommandLine::eventFilter(QObject* o, QEvent* e) {
    if (o == input_ && e->type() == QEvent::KeyPress) {
        auto* k = static_cast<QKeyEvent*>(e);
        if (k->key() == Qt::Key_Up && !history_.isEmpty()) {
            pos_ = qMax(0, pos_ - 1);
            input_->setText(history_[pos_]);
            return true;
        }
        if (k->key() == Qt::Key_Down && !history_.isEmpty()) {
            pos_ = qMin(int(history_.size()), pos_ + 1);
            input_->setText(pos_ < history_.size() ? history_[pos_] : QString());
            return true;
        }
    }
    return QWidget::eventFilter(o, e);
}

// ---------------------------------------------------------------------------------------------------------------------
// Hotbox

Hotbox::Hotbox(const QList<QMenu*>& menus, QWidget* parent) : QWidget(parent, Qt::Popup | Qt::FramelessWindowHint) {
    setAttribute(Qt::WA_TranslucentBackground);
    auto* grid = new QGridLayout(this);
    grid->setSpacing(4);
    int cols = 7;
    for (int i = 0; i < menus.size(); ++i) {
        auto* b = new QToolButton(this);
        b->setText(menus[i]->title().remove('&'));
        b->setMenu(menus[i]);
        b->setPopupMode(QToolButton::InstantPopup);
        b->setObjectName("hotboxButton");
        b->setMinimumWidth(96);
        grid->addWidget(b, i / cols, i % cols);
    }
}

void Hotbox::popup(QPoint global) {
    adjustSize();
    move(global - QPoint(width() / 2, height() / 2));
    show();
}

void Hotbox::paintEvent(QPaintEvent*) {
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing);
    p.setBrush(QColor(40, 40, 40, 200));
    p.setPen(Qt::NoPen);
    p.drawRoundedRect(rect(), 6, 6);
}

// ---------------------------------------------------------------------------------------------------------------------
// Preferences

Preferences::Preferences(QWidget* parent) : QDialog(parent) {
    setWindowTitle("Preferences");
    QSettings s;
    auto* lay = new QHBoxLayout(this);
    auto* cats = new QListWidget(this);
    cats->addItems({"Interface", "Display", "Settings", "Undo"});
    cats->setFixedWidth(130);
    auto* pages = new QStackedWidget(this);
    lay->addWidget(cats);
    auto* right = new QVBoxLayout();
    right->addWidget(pages, 1);
    lay->addLayout(right, 1);

    // Interface
    auto* ui = new QWidget(pages);
    auto* uf = new QFormLayout(ui);
    auto* font_size = new QSpinBox(ui);
    font_size->setRange(7, 16);
    font_size->setValue(s.value("prefs/fontSize", 9).toInt());
    uf->addRow("Interface font size (pt):", font_size);
    auto* help = new QCheckBox(ui);
    help->setChecked(s.value("prefs/helpLine", true).toBool());
    uf->addRow("Show help line:", help);
    pages->addWidget(ui);

    // Display
    auto* disp = new QWidget(pages);
    auto* df = new QFormLayout(disp);
    auto* bg = new QDoubleSpinBox(disp);
    bg->setRange(0, 1);
    bg->setSingleStep(0.02);
    bg->setDecimals(3);
    bg->setValue(s.value("prefs/background", 0.36).toDouble());
    df->addRow("Viewport background (grey):", bg);
    auto* manip = new QDoubleSpinBox(disp);
    manip->setRange(0.25, 4);
    manip->setSingleStep(0.25);
    manip->setValue(s.value("prefs/manipSize", 1.0).toDouble());
    df->addRow("Manipulator size:", manip);
    pages->addWidget(disp);

    // Settings
    auto* set = new QWidget(pages);
    auto* sf = new QFormLayout(set);
    auto* ch = new QCheckBox(set);
    ch->setChecked(s.value("prefs/constructionHistory", true).toBool());
    sf->addRow("Construction history on at start:", ch);
    auto* units = new QComboBox(set);
    units->addItem("centimeter");
    units->setEnabled(false);
    sf->addRow("Working units, linear:", units);
    auto* up = new QComboBox(set);
    up->addItem("Y");
    up->setEnabled(false);
    sf->addRow("World coordinate system, up axis:", up);
    pages->addWidget(set);

    // Undo
    auto* undo = new QWidget(pages);
    auto* unf = new QFormLayout(undo);
    auto* infinite = new QCheckBox(undo);
    infinite->setChecked(true);
    infinite->setEnabled(false);
    unf->addRow("Infinite undo:", infinite);
    pages->addWidget(undo);

    connect(cats, &QListWidget::currentRowChanged, pages, &QStackedWidget::setCurrentIndex);
    cats->setCurrentRow(0);
    auto* bb = new QDialogButtonBox(QDialogButtonBox::Save | QDialogButtonBox::Cancel, this);
    right->addWidget(bb);
    connect(bb, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(bb, &QDialogButtonBox::accepted, this, [=] {
        QSettings s2;
        s2.setValue("prefs/fontSize", font_size->value());
        s2.setValue("prefs/helpLine", help->isChecked());
        s2.setValue("prefs/background", bg->value());
        s2.setValue("prefs/manipSize", manip->value());
        s2.setValue("prefs/constructionHistory", ch->isChecked());
        emit changed();
        accept();
    });
    resize(620, 360);
}

} // namespace az
