# Mapping Translators

A simpler variant of [decoders](decoders) is the mapping translator.
This can convert bit-vector values into text using a simple configuration file.

The configuration files are located either in `.surfer/mappings/` in the current working directory or in any of the following, OS-dependent, directories:

| Os      | Path                                                                            |
|---------|---------------------------------------------------------------------------------|
| Linux   | `~/.config/surfer/mappings/`                                                    |
| Windows | `C:\Users\<Name>\AppData\Roaming\surfer-project\surfer\config\mappings\`        |
| macOS   | `/Users/<Name>/Library/Application Support/org.surfer-project.surfer/mappings/` |

The file format is in its simplest form just pairs of vector values and text strings, one per line.
For example:

``` text
0b0000 Start
0b0001 State 1
0b0010 State 2
0b0011 State 3
```

will map the binary variable value `0000` into the text `Start` and the variable value `0001` into the text `State2` and so on.

The values can be written either as binary (`0b`), octal (`0o`), decimal (no prefix) or hex (`0x`) and different formats can be used for each row.
It is possible to use `_` as part of the numbers to obtain clearer formatting.

Hence, the example above can be written as

``` text
0b00_00 Start
0x1 State 1
0o2 State 2
3 State 3
```

It is also possible to write values using 4-state or 9-state logic when using binary radix, so `0bxx01zz` is a valid value.

The wordlength is determined by the values, the longest string or binary value, or the number of bits required to represent the largest number.
It can also be specified using a line as the first or second line (see below about naming) as

``` text
Bits = 4
0 Start
1 State 1
2 State 2
3 State 3
```

It is also possible to name the translator, which then is used as the Format to select, by having a first line as

``` text
Name = My statemachine
Bits = 4
0 Start
1 State 1
2 State 2
3 State 3
```

If no name is provided, the filename without extension is used as Format name.

Finally, it is possible to supply a variable kind or color for each line.
This is supplied within `[]` directly after the value (no space).
The supported kinds and corresponding theme color names are:

| Kind       | Theme color         |
|------------|---------------------|
| `default`  | `variable_default`  |
| `dontcare` | `variable_dontcare` |
| `error`    | `variable_error`    |
| `event`    | `variable_event`    |
| `highimp`  | `variable_highimp`  |
| `normal`   | `variable_default`  |
| `undef`    | `variable_undef`    |
| `weak`     | `variable_weak`     |

For colors, any named [Color32](https://docs.rs/ecolor/latest/ecolor/struct.Color32.html) can be used (case insensitive, so `RED`, `red`, and `Red` will all work).
It is also possible to specify an RGD hex color, either with or without a leading `#` so both `aa7035`  and `#aa7035` are valid options.

If no kind/color is given, the waveform is drawn as a default/normal variable.

Comments starts with `#` and spans the rest of the line.

A final example including comments, kinds, and colors is:

``` text
# The statemachine in block 2
Name = Block 2 statemachine
Bits = 4
# Make the start state pink
0[pink] Start
# Often better to use the kinds as they follow the themes
# We also want to highlight State 1
1[error] State 1
# Empty lines are OK

2 State 2
3[#abcdef] State 3
```
