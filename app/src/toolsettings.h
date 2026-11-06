// The Tool Settings window: the current tool's options. A page per tool, as Maya's double-click on a tool opens.
#pragma once
#include "panels.h"
#include <QHash>
#include <QJsonArray>
#include <functional>

class QCheckBox;
class QComboBox;
class QFormLayout;
class QDoubleSpinBox;
class QListWidget;
class QStackedLayout;
class QButtonGroup;
class QSlider;

namespace az {

class ToolSettings : public Panel {
    Q_OBJECT
public:
    explicit ToolSettings(QWidget* parent = nullptr);
    void set_tool(const QString& tool);
protected:
    void refresh() override;
private:
    QWidget* build_transform_page();
    QWidget* build_paint_page();
    QWidget* build_joint_page();
    void refresh_paint();
    void refresh_transform();
    void paint_set(const QString& flag, const QJsonValue& v);
    QDoubleSpinBox* slider_row(QFormLayout* form, const QString& label, double lo, double hi, double step, std::function<void(double)> on);

    QString tool_ = "select";
    QStackedLayout* stack_ = nullptr;
    QLabel* title_ = nullptr;
    QWidget* transform_page_ = nullptr;
    QWidget* paint_page_ = nullptr;
    QWidget* joint_page_ = nullptr;
    // Paint page.
    QListWidget* influences_ = nullptr;
    QButtonGroup* op_ = nullptr;
    QComboBox* stamp_ = nullptr;
    QDoubleSpinBox* value_ = nullptr;
    QDoubleSpinBox* opacity_ = nullptr;
    QDoubleSpinBox* radius_ = nullptr;
    QCheckBox* feedback_ = nullptr;
    QString last_skin_;
    QStringList last_influences_;
    // Transform page.
    QCheckBox* soft_ = nullptr;
    QDoubleSpinBox* soft_radius_ = nullptr;
    QComboBox* soft_mode_ = nullptr;
    QComboBox* axis_ = nullptr;
    bool loading_ = false;
signals:
    void axisOrientation(const QString& mode);
};

} // namespace az
