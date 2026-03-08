# Property Inspector (PI) Reference

## sdpi-components v4

We use **sdpi-components v4** — a local copy at `pi/sdpi-components.js`.
Never reference a CDN or external URL.

### Minimal HTML template

```html
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8" />
    <script src="sdpi-components.js"></script>
</head>
<body>
    <!-- your components here -->
</body>
</html>
```

No separate CSS file needed — v4 bundles its own styles.

---

## How the PI connects to the plugin

Stream Deck invokes `connectElgatoStreamDeckSocket` on `window` after the DOM loads.
**sdpi-components v4 handles all of this automatically.** Just include the script tag and
use the custom elements — no manual WebSocket setup needed.

---

## Components Quick Reference

### Text Field
```html
<sdpi-item label="Name">
    <sdpi-textfield setting="userName" placeholder="Enter name"></sdpi-textfield>
</sdpi-item>
```

### Number Field
```html
<sdpi-item label="Count">
    <sdpi-textfield setting="count" type="number" placeholder="0"></sdpi-textfield>
</sdpi-item>
```

### Select (Static Options)
```html
<sdpi-item label="Mode">
    <sdpi-select setting="mode">
        <option value="a" selected>Option A</option>
        <option value="b">Option B</option>
    </sdpi-select>
</sdpi-item>
```

### Select (Dynamic Datasource)
```html
<sdpi-item label="Action">
    <sdpi-select setting="actionId" datasource="getActions"
        loading="Loading..." placeholder="Select an action" hot-reload>
    </sdpi-select>
```

The plugin handles datasource requests in `did_receive_sdpi_request`:
```rust
fn did_receive_sdpi_request(&mut self, cx: &Context, req: &DataSourceRequest<'_>) {
    if req.event == "getActions" {
        cx.sdpi().reply(req, vec![
            DataSourceResultItem::Item(DataSourceItem {
                disabled: None,
                label: Some("My Action".into()),
                value: "action_id".into(),
            }),
        ]);
    }
}
```

### Checkbox
```html
<sdpi-item label="Enabled">
    <sdpi-checkbox setting="enabled"></sdpi-checkbox>
</sdpi-item>
```

### Range / Slider
```html
<sdpi-item label="Volume">
    <sdpi-range setting="volume" min="0" max="100" step="1" default="50"></sdpi-range>
</sdpi-item>
```

### Color Picker
```html
<sdpi-item label="Color">
    <sdpi-color setting="color" default="#ffffff"></sdpi-color>
</sdpi-item>
```

### File Picker
```html
<sdpi-item label="Config File">
    <sdpi-file setting="configPath" accept="application/json"></sdpi-file>
</sdpi-item>
```

### Textarea
```html
<sdpi-item label="Notes">
    <sdpi-textarea setting="notes" rows="4" placeholder="Enter notes..."></sdpi-textarea>
</sdpi-item>
```

### Horizontal Rule (Separator)
```html
<hr />
```

---

## Key Attributes

| Attribute | Purpose |
|-----------|---------|
| `setting="key"` | Auto-syncs with Stream Deck per-key settings |
| `global` | Syncs with global settings instead of per-key |
| `datasource="eventName"` | Dynamic options fetched from plugin |
| `hot-reload` | Re-fetches datasource when PI re-opens |
| `value-type="number"` | Coerces value to number before saving |
| `disabled` | Disables the control |
| `required` | Marks the field as required |
| `pattern="regex"` | Validation pattern for text fields |
| `placeholder="text"` | Placeholder text |
| `default="value"` | Default value |

---

## Receiving messages from the plugin

When the plugin calls `cx.sd().send_to_property_inspector(ctx_id, payload)`:

```js
SDPIComponents.streamDeckClient.sendToPropertyInspector.subscribe((msg) => {
    const data = msg.payload;
    // handle data from plugin
});
```

## Sending messages to the plugin

```js
SDPIComponents.streamDeckClient.send('sendToPlugin', { action: 'refresh' });
```

The plugin receives this in `did_receive_property_inspector_message`.

---

## DataSource Types (Rust side)

```rust
// Simple item
DataSourceResultItem::Item(DataSourceItem {
    disabled: None,           // Option<bool>
    label: Some("Label".into()), // Option<String>
    value: "value".into(),    // String (required)
})

// Grouped items
DataSourceResultItem::Group(DataSourceGroup {
    label: Some("Group Name".into()),
    children: vec![
        DataSourceItem { disabled: None, label: Some("A".into()), value: "a".into() },
        DataSourceItem { disabled: None, label: Some("B".into()), value: "b".into() },
    ],
})
```

---

## Resources

- sdpi-components homepage: https://sdpi-components.dev
- Component reference: https://sdpi-components.dev/docs/components
- Stream Deck SDK: https://docs.elgato.com/sdk/plugins/overview
