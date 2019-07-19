### Overview

[Roguelikedev Does The Complete Roguelike Tutorial 2019](https://old.reddit.com/r/roguelikedev/comments/bz6s0j/roguelikedev_does_the_complete_roguelike_tutorial/)


[Rust version of the tutorial](https://tomassedovic.github.io/roguelike-tutorial/part-2-object-map.html)

### Requirements

Builds requires libsdl2-dev.

Font Bisasam_16x16.png taken from [Dwarf Fortress wiki](https://dwarffortresswiki.org/index.php/File:Bisasam_16x16.png).
Font Cheepicus_15x15.png taken from [Dwarf Fortress wiki](https://dwarffortresswiki.org/Tileset_repository).
Font courier12x12_aa_tc taken from [libtcod-rs](https://github.com/bhelyer/libtcod-d/tree/master/data/fonts)


### Usage

Run:

> cargo run --release  # to use default tcod font
> cargo run FONT_NAME  # to use some other font

### TODO


### Bugs

* Player can be generated on the same tile as monster
