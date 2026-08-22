#!/usr/bin/env python3
"""Convert a `cva()` definition (TypeScript) into Rust `#[tw(...)]` types.

This is a helper for the upio GUI frontend: it turns a CVA variant object into
a Rust struct plus one `TwVariant` enum per variant axis, matching the `tw`
attribute syntax. The generated Rust is copied to your clipboard.

Paste a cva definition and copy the result to the clipboard:

    # Pipe a pasted cva in:
    python cva_to_rust.py < button.cva.ts
    # ...or paste directly as the argument:
    python cva_to_rust.py 'export const buttonVariants = cva( ... );'

    # Pick the struct name:
    python cva_to_rust.py -n ButtonStyles < button.cva.ts

    # Also print the result to the terminal:
    python cva_to_rust.py -p < button.cva.ts

To see the output without touching the clipboard, use `--print-only`.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field
from typing import Any, NoReturn, Optional


class _JSValueParser:
    """Parse a JSON-like JS object value into Python objects."""

    def __init__(self, text: str) -> None:
        self.text = text
        self.pos = 0

    def _skip_ws(self) -> None:
        while self.pos < len(self.text) and self.text[self.pos].isspace():
            self.pos += 1

    def _peek(self) -> str:
        self._skip_ws()
        return self.text[self.pos] if self.pos < len(self.text) else ""

    def _error(self, msg: str) -> NoReturn:
        raise ValueError(f"{msg} (at char {self.pos})")

    def _read_string(self) -> str:
        quote = self.text[self.pos]
        self.pos += 1
        out: list[str] = []
        while self.pos < len(self.text):
            ch = self.text[self.pos]
            if ch == quote:
                self.pos += 1
                return "".join(out)
            if ch == "\\":
                self.pos += 1
                if self.pos >= len(self.text):
                    break
                esc = self.text[self.pos]
                mapping = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", "\\": "\\", "'": "'", '"': '"'}
                out.append(mapping.get(esc, esc))
                self.pos += 1
                continue
            out.append(ch)
            self.pos += 1
        self._error("unterminated string")

    def _read_ident(self) -> str:
        start = self.pos
        while self.pos < len(self.text) and (self.text[self.pos].isalnum() or self.text[self.pos] in "_$"):
            self.pos += 1
        return self.text[start : self.pos]

    def _parse_value(self) -> Any:
        ch = self._peek()
        if ch in "\"'":
            return self._read_string()
        if ch == "{":
            return self._parse_object()
        if ch == "[":
            return self._parse_array()
        if ch == "-" or ch.isdigit():
            return self._parse_number()
        # identifiers / keywords
        ident = self._read_ident()
        if ident in ("true", "True"):
            return True
        if ident in ("false", "False"):
            return False
        if ident in ("null", "undefined", "None"):
            return None
        if ident:
            return ident  # bare identifier, e.g. an unquoted key value
        self._error(f"unexpected character {ch!r}")

    def _parse_number(self) -> float:
        start = self.pos
        if self._peek() == "-":
            self.pos += 1
        while self.pos < len(self.text) and (self.text[self.pos].isdigit() or self.text[self.pos] in ".eE+-"):
            self.pos += 1
        try:
            return float(self.text[start : self.pos])
        except ValueError:
            self._error(f"invalid number {self.text[start : self.pos]!r}")

    def _parse_object(self) -> dict:
        obj: dict = {}
        self.pos += 1  # consume {
        while True:
            self._skip_ws()
            ch = self._peek()
            if ch == "}":
                self.pos += 1
                return obj
            if ch in "\"'":
                key = self._read_string()
            else:
                key = self._read_ident()
                if not key:
                    self._error("expected object key")
            self._skip_ws()
            if self._peek() != ":":
                self._error("expected ':' after object key")
            self.pos += 1
            value = self._parse_value()
            obj[key] = value
            self._skip_ws()
            ch = self._peek()
            if ch == ",":
                self.pos += 1
            elif ch == "}":
                self.pos += 1
                return obj
            else:
                self._error("expected ',' or '}' in object")

    def _parse_array(self) -> list:
        arr: list = []
        self.pos += 1  # consume [
        while True:
            self._skip_ws()
            ch = self._peek()
            if ch == "]":
                self.pos += 1
                return arr
            arr.append(self._parse_value())
            self._skip_ws()
            ch = self._peek()
            if ch == ",":
                self.pos += 1
            elif ch == "]":
                self.pos += 1
                return arr
            else:
                self._error("expected ',' or ']' in array")


def _parse_js_value(text: str) -> Any:
    parser = _JSValueParser(text.strip())
    value = parser._parse_value()
    parser._skip_ws()
    if parser.pos != len(parser.text):
        parser._error("trailing content after value")
    return value


def _camel_to_pascal(name: str) -> str:
    """Convert a kebab/snake/camel name to PascalCase.

    `"icon-lg"` -> `IconLg`, `"icon_xl"` -> `IconXl`.
    """
    parts = re.split(r"[-_\s]+", name.strip())
    parts = [p for p in parts if p]
    if not parts:
        return "Default"
    return "".join(p[0].upper() + p[1:] for p in parts if p)


def _safe_rust_ident(name: str) -> str:
    """Escape identifiers that are Rust keywords."""
    keywords = {
        "as",
        "break",
        "const",
        "continue",
        "crate",
        "else",
        "enum",
        "extern",
        "false",
        "fn",
        "for",
        "if",
        "impl",
        "in",
        "let",
        "loop",
        "match",
        "mod",
        "move",
        "mut",
        "pub",
        "ref",
        "return",
        "self",
        "Self",
        "static",
        "struct",
        "super",
        "trait",
        "true",
        "type",
        "unsafe",
        "use",
        "where",
        "while",
        "async",
        "await",
        "dyn",
        "abstract",
        "become",
        "box",
        "do",
        "final",
        "macro",
        "override",
        "priv",
        "typeof",
        "unsized",
        "virtual",
        "yield",
        "try",
    }
    if name in keywords:
        return f"{name}_"
    return name


@dataclass
class VariantAxis:
    name: str
    values: "list[VariantValue]" = field(default_factory=list)


@dataclass
class VariantValue:
    key: str  # raw cva key, e.g. "icon-lg"
    rust_variant: str  # PascalCase Rust enum variant, e.g. "IconLg"
    class_str: str  # the Tailwind classes


@dataclass
class CvaModel:
    base_class: str
    axes: "list[VariantAxis]"
    default_variants: dict[str, str]  # axis name -> default value key
    struct_name: str
    export_name: str


def _extract_cva_call(text: str) -> str:
    """Extract the argument list of the first `cva(` call."""
    idx = text.find("cva(")
    if idx == -1:
        raise ValueError("no `cva(` call found")
    start = text.index("(", idx) + 1
    depth = 1
    i = start
    while i < len(text) and depth > 0:
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
        i += 1
    if depth != 0:
        raise ValueError("unbalanced parentheses in cva() call")
    return text[start : i - 1]


def _split_top_level(text: str) -> "list[str]":
    """Split on top-level commas (respecting brackets, parens, quotes)."""
    parts: list[str] = []
    depth = 0
    quote: Optional[str] = None
    buf: list[str] = []
    for ch in text:
        if quote:
            buf.append(ch)
            if ch == quote:
                quote = None
            continue
        if ch in "\"'`":
            quote = ch
            buf.append(ch)
            continue
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(buf))
            buf = []
        else:
            buf.append(ch)
    if quote:
        raise ValueError("unterminated string in cva()")
    parts.append("".join(buf))
    return parts


def _parse_cva(text: str, struct_name: str) -> CvaModel:
    args = _split_top_level(_extract_cva_call(text))
    if len(args) < 2:
        raise ValueError("cva() needs at least a base class and an options object")

    base_class = args[0].strip()
    base_class = base_class[1:-1] if base_class[:1] in "\"'" else base_class

    options = _parse_js_value(args[1])
    if not isinstance(options, dict):
        raise ValueError("cva() second argument must be an object")

    variants = options.get("variants") or {}
    default_variants = options.get("defaultVariants") or {}

    axes: list[VariantAxis] = []
    for axis_name, axis_values in variants.items():
        if not isinstance(axis_values, dict):
            raise ValueError(f"variants.{axis_name} must be an object")
        axis = VariantAxis(name=str(axis_name))
        for value_key, class_str in axis_values.items():
            axis.values.append(
                VariantValue(
                    key=str(value_key),
                    rust_variant=_safe_rust_ident(_camel_to_pascal(str(value_key))),
                    class_str=str(class_str),
                )
            )
        axes.append(axis)

    return CvaModel(
        base_class=base_class,
        axes=axes,
        default_variants={str(k): str(v) for k, v in default_variants.items()},
        struct_name=struct_name,
        export_name="buttonVariants",
    )


def _infer_struct_name(text: str) -> str:
    """Guess a struct name from the export/const name, e.g. buttonVariants."""
    m = re.search(r"export\s+const\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*cva\(", text)
    if m:
        return _camel_to_pascal(m.group(1))
    m = re.search(r"const\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*cva\(", text)
    if m:
        return _camel_to_pascal(m.group(1))
    return "Styles"


def _axis_enum_name(axis_name: str, struct_name: str) -> str:
    """Derive the enum name for an axis, e.g. `ButtonSize` / `ButtonVariant`.

    A struct like `ButtonVariants` or `ButtonStyles` contributes its root
    (`Button`), which is combined with the axis name so we don't get redundant
    names such as `ButtonVariantsSize`.
    """
    root = struct_name
    for suffix in ("Variants", "Styles", "Variant", "Style"):
        if root.endswith(suffix) and len(root) > len(suffix):
            root = root[: -len(suffix)]
            break
    return f"{root}{_camel_to_pascal(axis_name)}"


def _emit_struct(model: CvaModel) -> str:
    lines: list[str] = []
    lines.append("#[derive(TwClass)]")
    lines.append("")
    base = model.base_class
    base_esc = base.replace("\\", "\\\\").replace('"', '\\"')
    lines.append(f'#[tw(class = "{base_esc}")]')
    lines.append(f"pub struct {model.struct_name} {{")
    for axis in model.axes:
        enum_name = _axis_enum_name(axis.name, model.struct_name)
        lines.append(f"    {_safe_rust_ident(axis.name)}: {enum_name},")
    lines.append("}")
    return "\n".join(lines)


def _emit_enum(model: CvaModel, axis: VariantAxis) -> str:
    enum_name = _axis_enum_name(axis.name, model.struct_name)
    default_key = model.default_variants.get(axis.name)
    lines: list[str] = []
    lines.append("#[derive(PartialEq, TwVariant)]")
    lines.append(f"pub enum {enum_name} {{")
    for value in axis.values:
        is_default = value.key == default_key
        quoted = value.class_str.replace("\\", "\\\\").replace('"', '\\"')
        if is_default:
            attr = f'#[tw(default, class = "{quoted}")]'
        else:
            attr = f'#[tw(class = "{quoted}")]'
        lines.append(f"    {attr}")
        lines.append(f"    {value.rust_variant},")
    lines.append("}")
    return "\n".join(lines)


def convert(text: str, struct_name: Optional[str] = None) -> str:
    struct_name = struct_name or _infer_struct_name(text)
    model = _parse_cva(text, struct_name)

    chunks: list[str] = []
    chunks.append(_emit_struct(model))
    for axis in model.axes:
        chunks.append(_emit_enum(model, axis))
    return "\n\n".join(chunks) + "\n"


def _cva_end_index(text: str) -> Optional[int]:
    """Return the index just past a complete top-level `cva(...);` call, or None.

    Finds `cva(`, tracks paren nesting, and only considers the call complete
    once it is balanced and the closing `)` is followed by `;` (or only
    whitespace then `;`). Used to know when a pasted snippet is finished.
    """
    start = text.find("cva(")
    if start == -1:
        return None
    # `cva(` is 4 chars; index start+3 is the opening '('.
    i = start + 3
    depth = 0
    while i < len(text):
        ch = text[i]
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                # Look ahead for a terminating semicolon.
                j = i + 1
                while j < len(text) and text[j] in " \t\r\n":
                    j += 1
                if j < len(text) and text[j] == ";":
                    return j + 1
                return None
        i += 1
    return None


def _read_stdin_cva() -> str:
    """Read stdin until a complete `cva(...);` has been pasted.

    Plain `sys.stdin.read()` blocks forever in interactive terminals (PowerShell
    never sends EOF on a paste). Reading line by line and stopping once the
    call is balanced and terminated lets the user just paste and go.
    """
    buf: list[str] = []
    while True:
        try:
            line = sys.stdin.readline()
        except KeyboardInterrupt:
            break
        if not line:  # EOF
            break
        buf.append(line)
        if _cva_end_index("".join(buf)) is not None:
            break
    return "".join(buf)


def _copy_to_clipboard(text: str) -> bool:
    """Copy text to the system clipboard using the platform's tool.

    Returns True on success.
    """
    data = text.encode("utf-8")
    try:
        if sys.platform == "win32":
            # Windows: `clip` is available everywhere.
            subprocess.run(["clip"], input=data, check=True)
            return True
        if sys.platform == "darwin":
            subprocess.run(["pbcopy"], input=data, check=True)
            return True
        # Linux: prefer wl-copy (Wayland), then xclip (X11).
        for tool in ("wl-copy", "xclip"):
            if subprocess.run(["which", tool], capture_output=True).returncode == 0:
                if tool == "xclip":
                    subprocess.run(["xclip", "-selection", "clipboard"], input=data, check=True)
                else:
                    subprocess.run([tool], input=data, check=True)
                return True
    except (OSError, subprocess.SubprocessError):
        return False
    return False


def main(argv: "list[str]") -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "input",
        nargs="?",
        default=None,
        help="Path to a .ts file, or the raw cva definition pasted inline. If omitted, reads from stdin.",
    )
    parser.add_argument(
        "-n",
        "--name",
        dest="name",
        default=None,
        help="Rust struct name (defaults to the cva export/const name)",
    )
    parser.add_argument(
        "-p",
        "--print",
        dest="print_",
        action="store_true",
        help="Also print the generated Rust to stdout",
    )
    parser.add_argument(
        "--print-only",
        dest="print_only",
        action="store_true",
        help="Print to stdout and skip copying to the clipboard",
    )
    args = parser.parse_args(argv)

    # Read the input: a file path if it exists, otherwise treat it as raw
    # pasted text; fall back to stdin (reading until a complete cva call) when
    # nothing was given.
    if args.input:
        if os.path.isfile(args.input):
            with open(args.input, "r", encoding="utf-8") as fh:
                text = fh.read()
        else:
            text = args.input
    else:
        text = _read_stdin_cva()

    try:
        out = convert(text, args.name)
    except ValueError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1

    if args.print_only:
        sys.stdout.write(out)
        return 0

    if _copy_to_clipboard(out):
        print(f"Copied {_summarize(out)} to the clipboard.", file=sys.stderr)
    else:
        print("Could not copy to clipboard; printing instead:", file=sys.stderr)
        sys.stdout.write(out)
        return 1

    if args.print_:
        sys.stdout.write(out)

    return 0


def _summarize(text: str) -> str:
    structs = sum(1 for line in text.splitlines() if line.startswith("pub struct "))
    enums = sum(1 for line in text.splitlines() if line.startswith("pub enum "))
    return f"{structs} struct + {enums} enums"


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
