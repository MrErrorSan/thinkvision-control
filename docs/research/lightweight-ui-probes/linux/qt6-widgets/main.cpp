// Qt 6 Widgets probe window: label, slider 0-100, checkbox (no stock switch), combo (3 items), button.
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QLabel>
#include <QPushButton>
#include <QSlider>
#include <QVBoxLayout>
#include <QWidget>

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    QWidget win;
    win.setWindowTitle("probe");
    win.resize(600, 400);
    auto *layout = new QVBoxLayout(&win);
    auto *label = new QLabel("Brightness");
    auto *slider = new QSlider(Qt::Horizontal);
    slider->setRange(0, 100);
    slider->setValue(50);
    auto *toggle = new QCheckBox("Enabled");
    auto *combo = new QComboBox;
    combo->addItems({"Standard", "Movie", "Game"});
    auto *button = new QPushButton("Apply");
    QObject::connect(button, &QPushButton::clicked, [label] { label->setText("clicked"); });
    layout->addWidget(label);
    layout->addWidget(slider);
    layout->addWidget(toggle);
    layout->addWidget(combo);
    layout->addWidget(button);
    layout->addStretch();
    win.show();
    return app.exec();
}
