// The smaller editor windows of the Windows and Modify menus, and the In-View Editor.
#pragma once
#include <QFrame>
#include <QString>

class QVBoxLayout;

namespace az {

// A window by id: spreadSheet, componentEditor, connectionEditor, hypergraphHierarchy, hypergraphConnections,
// namespaceEditor, channelControl, modelingToolkit, shapeEditor, expressionEditor, polySelectConstraint, alignObjects,
// searchReplace, addAttribute, editAttribute, deleteAttribute, gridOptions. Null for an id it doesn't know.
QWidget* make_editor_window(const QString& id, QWidget* parent);

// Whether the core has a command, for items that stay disabled until it does.
bool has_command_named(const QString& name);

// A small panel over the view with the last history node's attributes, after an edit.
class InViewEditor : public QFrame {
    Q_OBJECT
public:
    explicit InViewEditor(QWidget* parent);
    // The newest history node of the lead selection; hides when there's none.
    void show_last();
private:
    QVBoxLayout* body_;
    QWidget* rows_ = nullptr;
};

} // namespace az
