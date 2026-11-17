# Nyx Maps

A lightweight, native Linux desktop mapping application built with Rust and GTK4. Part of the Nyx desktop suite, for NyxOS and Omarchy.

![Maps Icon](data/icons/io.github.design_nexus.nyx.Maps.svg)

---

## Features

- **Interactive Maps:** Smooth panning, touchpad zooming, mouse wheel navigation, and keyboard controls powered by `libshumate`.
- **Three Map Styles:**
  - **Standard:** OpenStreetMap data as sharp vector tiles from OpenFreeMap, drawn in your theme's light or dark colors.
  - **Topographic:** OpenTopoMap terrain and elevation contours.
  - **Satellite:** High-resolution aerial imagery via Esri World Imagery (or custom tile template).
- **Search & Autocomplete:** Search for street addresses, cities, landmarks, businesses, or direct coordinates (`lat, lon`).
- **Turn-by-Turn Directions:** Driving, walking, and cycling route calculation with route polylines, distance, and duration.
- **Current Location:** GeoClue integration for one-click desktop location detection.
- **Saved Places:** Fast local SQLite database for favorites, Home, Work, and recent searches.
- **Nyx Suite Integration:**
  - Follows Omarchy theme dynamically with live restyling.
  - Accessible to **Nyx Assistant** via headless CLI (`--api`) and D-Bus.
  - Deep-link support for **Nyx Glimpse** (`geo:` and `nyx-maps://` schemes).

---

## Installation

On NyxOS, install Maps from the Store. On Arch Linux or Omarchy:

### Prerequisites (Arch Linux / Omarchy)
```bash
sudo pacman -S libshumate gtk4 geoclue
```

### Install
```bash
curl -fsSL https://raw.githubusercontent.com/design-nexus/nyx-maps/main/install.sh | bash
```

From a checkout, `./install.sh` does the same.

---

## Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| <kbd>Ctrl</kbd>+<kbd>F</kbd> | Focus search bar |
| <kbd>Ctrl</kbd>+<kbd>B</kbd> | Toggle side panel |
| <kbd>Ctrl</kbd>+<kbd>R</kbd> | Open directions panel |
| <kbd>Ctrl</kbd>+<kbd>,</kbd> | Open settings |
| <kbd>+</kbd> / <kbd>=</kbd> | Zoom in |
| <kbd>-</kbd> | Zoom out |
| <kbd>F1</kbd> | Show shortcuts list |
| <kbd>Esc</kbd> | Close panels and cards |
| <kbd>Ctrl</kbd>+<kbd>Q</kbd> | Quit application |

---

## License

MIT License. See [LICENSE](LICENSE) for details.
