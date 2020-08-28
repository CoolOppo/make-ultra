# Make Ultra
Make Ultra is a task runner useful for running certain commands when your files change.

Check out the following rule file (written in [TOML](https://github.com/toml-lang/toml)):

```toml
# saved as makeultra.toml

folders = ["Foo", "Bar"]

[[rule]]
# rules use rusty regex: https://docs.rs/regex/*/regex/#syntax
from = '(?P<name>.*)\.js$'
to = '$name.min.js'
exclude = '\.min\.js$'
# For all commands: $i = input file; $o = output file
command = 'terser $i -o $o'

[[rule]]
from = '(?P<name>.*)\.min\.js$'
to = '$name.min.js.gz'
command = 'zopfli $i'

[[rule]]
from = '(?P<name>.*)\.min\.js$'
to = '$name.min.js.br'
command = 'brotli -f $i'

# Optimize png files in-place, only re-running when you modify them:
[[rule]]
from = '(?P<name>.*\.png)$'
to = '$name'
command = 'optipng -clobber -fix -quiet -strip all $i'
```

## Why Another?  
I needed something faster than Grunt and Gulp that had a simpler syntax than Make and tracking of files modified in-place. Make Ultra accomplishes these goals.

### Features:  
1. It doesn't require you to explicitly state dependencies.
	- *[Currently](https://github.com/CoolOppo/make-ultra/issues/6), you might have to use `exclude` to ensure your rules aren't overly-zealous. Automatically determining appropriate rules is on the roadmap.*
	- *On a side note: It's not exactly a beta build system, but Make Ultra borrows from the ideas of a [beta build system](http://gittup.org/tup/build_system_rules_and_algorithms.pdf). It processes dependencies first and recursively crawls down to their dependents.*
2. It can track whether files that are modified in-place need to be rebuilt.
	- *While this is possible with Make, it is [inconvenient](https://www.gnu.org/software/make/manual/make.html#Empty-Targets).*
3. File hashes are generated with [hashbrown](https://github.com/Amanieu/hashbrown)'s hasher (and hashbrown is used for all HashMaps interally), serialized with [bincode](https://github.com/TyOverby/bincode), and cached within `.make_cache`, allowing us to keep track of whether or not to rebuild files and their dependents without relying on filesystem metadata.
4. Rule files are written in [TOML](https://github.com/toml-lang/toml) and use [Rust's regex syntax](https://docs.rs/regex/*/regex/#syntax) to match and replace file patterns.
5. It's cross-platform.
6. It's very fast.
	- Uses [`WalkParallel`](https://docs.rs/ignore/0.4.6/ignore/struct.WalkParallel.html) to scan directories (the same as `fd` and `ripgrep`)
	- Multithreaded by default, automatically parallelizing tasks as much as possible
7. You can generate a [DOT](https://en.wikipedia.org/wiki/DOT_(graph_description_language)) file with the `--dot` option and view the build tree.

#### Still a Long Way to Go
More is in the works for this project to be worthy of its name, but I don't know if anything can ever beat Make. This also serves as a project for me to learn Rust while accomplishing something that hasn't been done yet in the language (*cargo-make* isn't language-agnostic and *just* doesn't have support for wildcards).

This project is not ready for a lot of use cases (e.g. [multiple dependencies for a single input file](https://github.com/CoolOppo/make-ultra/issues/4), so you can't link together your `.o` files yet). **Right now, it actually works great if you are simply running tasks on individual files**, but it lacks support for [the things](https://github.com/CoolOppo/make-ultra/issues) that are necessary for more elaborate workloads that will allow you to build larger projects.

------

## Other Similar, Language-Agnostic Tools
- [Make](https://www.gnu.org/software/make/) -- [It really might be all you need](https://bost.ocks.org/mike/make/)!
- [task](https://taskfile.org)
- [just](https://github.com/casey/just)
