#pragma once

#include <QString>
#include <QVariantMap>

// `streamdeck open FILE|URL`: what browsers and file managers run. It hands
// the file or link to streamdeckd without opening a window, then reports the
// result as a desktop notification, since nobody sees its terminal output.
namespace Opener {

struct Outcome {
    bool ok = true;
    QString summary;
    QString body;
};

// The marketplace item in streamdeck://plugins/install/<id>, or "" when the
// link is something else.
QString marketplaceItem(const QString &url);

// After installPlugin: `plugin` is its listPlugins entry.
Outcome pluginInstalled(const QVariantMap &plugin);

// For a marketplace link: `item` is its public marketplace record and
// `installed` the matching listPlugins entry, empty when not installed.
Outcome marketplaceLink(const QVariantMap &item, const QVariantMap &installed);

int run(const QString &target, const QString &socketPath);

} // namespace Opener
