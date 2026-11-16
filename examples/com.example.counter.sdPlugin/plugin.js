// A minimal Stream Deck plugin with no dependencies, written against the
// documented SDK protocol: register, then react to events by context.
const args = {};
for (let i = 2; i < process.argv.length; i += 2) args[process.argv[i].replace(/^-/, "")] = process.argv[i + 1];

const ws = new WebSocket(`ws://127.0.0.1:${args.port}`);
const send = (msg) => ws.send(JSON.stringify(msg));
const counts = new Map();

function show(context, controller) {
  const n = counts.get(context) ?? 0;
  if (controller === "Encoder") {
    send({ event: "setFeedback", context, payload: { title: "Count", value: String(n), indicator: { value: Math.max(0, Math.min(100, n)) } } });
  } else {
    send({ event: "setTitle", context, payload: { title: String(n) } });
  }
}

ws.onopen = () => {
  send({ event: args.registerEvent, uuid: args.pluginUUID });
  send({ event: "logMessage", payload: { message: "counter plugin connected" } });
};

ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  const ctx = m.context;
  const controller = m.payload?.controller;
  switch (m.event) {
    case "willAppear":
      counts.set(ctx, m.payload.settings.count ?? 0);
      show(ctx, controller);
      break;
    case "keyUp": {
      const n = (counts.get(ctx) ?? 0) + (m.payload.settings.step ?? 1);
      counts.set(ctx, n);
      send({ event: "setSettings", context: ctx, payload: { ...m.payload.settings, count: n } });
      show(ctx, "Keypad");
      if (n % 10 === 0) send({ event: "showOk", context: ctx });
      break;
    }
    case "dialRotate": {
      const n = (counts.get(ctx) ?? 0) + m.payload.ticks;
      counts.set(ctx, n);
      send({ event: "setSettings", context: ctx, payload: { ...m.payload.settings, count: n } });
      show(ctx, "Encoder");
      break;
    }
    case "dialDown":
      counts.set(ctx, 0);
      send({ event: "setSettings", context: ctx, payload: { ...m.payload.settings, count: 0 } });
      show(ctx, "Encoder");
      break;
    case "sendToPlugin":
      if (m.payload?.reset) {
        counts.set(ctx, 0);
        show(ctx, "Keypad");
      }
      send({ event: "sendToPropertyInspector", context: ctx, payload: { count: counts.get(ctx) ?? 0 } });
      break;
  }
};
