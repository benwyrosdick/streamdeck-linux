QT += core gui qml quick quickcontrols2 dbus network webenginequick webenginecore

CONFIG += c++17 release
TARGET = streamdeck
TEMPLATE = app

HEADERS += \
    src/apputil.h \
    src/daemonclient.h \
    src/opener.h \
    src/pluginhost.h \
    src/portalfilepicker.h \
    src/theme.h \
    src/translator.h

SOURCES += \
    src/main.cpp \
    src/apputil.cpp \
    src/daemonclient.cpp \
    src/opener.cpp \
    src/pluginhost.cpp \
    src/portalfilepicker.cpp \
    src/theme.cpp \
    src/translator.cpp

RESOURCES += src/resources.qrc
