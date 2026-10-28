// Probe: Qt 6 Widgets (dynamic). Label, slider 0-100, checkbox (no switch in Widgets), combo (3 items), button.
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QLabel>
#include <QPushButton>
#include <QSlider>
#include <QVBoxLayout>
#include <QWidget>

int main(int argc, char *argv[])
{
    QApplication app(argc, argv);
    QWidget w;
    w.setWindowTitle("probe");
    w.resize(600, 400);
    auto *l = new QVBoxLayout(&w);
    l->setContentsMargins(24, 24, 24, 24);
    l->setSpacing(12);
    l->addWidget(new QLabel("Brightness"));
    auto *s = new QSlider(Qt::Horizontal);
    s->setRange(0, 100);
    s->setValue(50);
    l->addWidget(s);
    l->addWidget(new QCheckBox("Auto"));
    auto *c = new QComboBox;
    c->addItems({"sRGB", "DCI-P3", "Custom"});
    l->addWidget(c);
    l->addWidget(new QPushButton("Apply"));
    l->addStretch();
    w.show();
    return app.exec();
}
