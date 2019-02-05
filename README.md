MakeUltra is a task runner and build tool.

## Why Another?  
I needed something faster than Grunt and Gulp that had a simpler syntax than Make. MakeUltra accomplishes both of these goals.

### Better than the Rest  
1. It doesn't require you to explicitly state dependencies.
2. In-place modification of files is supported.
	- *While this is possible with Make, it is [sort-of hacky](https://www.gnu.org/software/make/manual/make.html#Empty-Targets).*
3. Rules use regex to match file patterns.

Check out the following rule file:

```toml
[minify]
from = '(?P<name>.*)\.js$'
to = '$name.min.js'
exclude = '\.min\.js$'
command = 'terser $i -o $o'

[gzip]
from = '(?P<name>.*)\.min\.js$'
to = '$name.min.js.gz'
command = 'zopfli $i'

[brotli]
from = '(?P<name>.*)\.min\.js$'
to = '$name.min.js.br'
command = 'brotli -f $i'


[png]
from = '(?P<name>.*\.png)$'
to = '$name'
command = 'optipng -clobber -fix -quiet -strip all $i'
```
