# Example asset credits

These files are used only by the bundled examples (`basic_world`, `minetest`).
The library itself does not load anything from `assets/`.

## Textures

`assets/*/textures/*.png` are original 32x32 textures generated procedurally
by [`scripts/generate-example-textures.py`](../scripts/generate-example-textures.py)
(requires Pillow). They are part of this project and covered by its
LGPL-3.0-or-later license. To regenerate:

```sh
python3 scripts/generate-example-textures.py assets
```

## Audio

`assets/*/audio/*.ogg` come from Kenney's sound packs
(Impact Sounds, Interface Sounds / UI Audio, Sci-fi Sounds), released under
CC0 1.0 Universal by Kenney (https://kenney.nl).
