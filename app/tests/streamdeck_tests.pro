QT += core gui qml quick quickcontrols2 testlib dbus network
CONFIG += c++17 testcase
TARGET = streamdeck_tests
TEMPLATE = app

INCLUDEPATH += ../src

HEADERS += ../src/daemonclient.h ../src/theme.h ../src/translator.h ../src/apputil.h
RESOURCES += ../src/resources.qrc
SOURCES += streamdeck_tests.cpp ../src/daemonclient.cpp ../src/theme.cpp ../src/translator.cpp ../src/apputil.cpp
