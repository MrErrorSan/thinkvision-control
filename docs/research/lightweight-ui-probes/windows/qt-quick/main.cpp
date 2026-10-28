// Probe: Qt 6 Quick/QML (dynamic), default Quick Controls style for the platform.
#include <QGuiApplication>
#include <QQmlApplicationEngine>

int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);
    QQmlApplicationEngine engine;
    engine.loadFromModule("Probe", "Main");
    return engine.rootObjects().isEmpty() ? 1 : app.exec();
}
