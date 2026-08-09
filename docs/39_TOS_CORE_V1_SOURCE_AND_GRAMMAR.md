<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — source model and grammar

- Status: **Accepted Tier 2 contract — production implementation in progress**
- Language version: `TOS Core 1.0`
- Authority on acceptance: Tier 2 under
  `docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`
- Governing Tier 1 decision: ADR-0027
- Companion contracts: `docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md`,
  `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`,
  `docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md`,
  `docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md`, and
  `docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md`

## Status and boundary

This document is the accepted lexical and syntactic part of one TOS Core V1
contract set. It is intentionally detailed enough to prevent a first parser
from inventing language semantics. ADR-0028 accepts it as Tier 2 authority
under the normative hierarchy. It authorizes the production reference
implementation; implementation status remains separate and is recorded in the
guide, tutorial, conformance evidence, and stage report.

TOS Core V1 is the TOS-owned textual language selected by ADR-0027. Canonical
installed code is normalized UTF-8 `.tos` source. ASTs, typed IR, bytecode and
native code are derived artifacts. This specification defines language syntax;
it does not make an existing host compiler, C ABI, host thread API, LLVM, Rust,
Wasm, libc or external VM part of the TOS contract.

## 1. Canonical source unit

A source unit is exactly one file with extension `.tos` and one `module`
declaration. Its canonical identity consists of:

```text
source_set_identity
canonical repository path
sha256(normalized_source_bytes)
language version (1.0)
profile declaration
```

`source_set_identity` is the active commit identity or an explicitly accepted
detached source-set identity; it is not a pathname, working directory, clock,
network response, random value, or host environment variable. The SHA-256
value is written `sha256:<lowercase-hex>` and identifies normalized source
bytes, not an executable derivative.

A canonical source unit MUST:

- be valid UTF-8;
- be Unicode NFC after newline normalization;
- contain no UTF-8 BOM;
- use LF (`U+000A`) line endings; and
- contain no NUL scalar value.

For TOS Core 1.0, **Unicode NFC** means NFC exactly under Unicode Standard and
Unicode Character Database (UCD) **17.0.0**, using UAX #15 Revision 57. This
fixed normalization baseline is part of the source-language version, not a
host-library, locale, operating-system, or implementation choice. A newer
Unicode release MUST NOT silently change TOS Core 1.0 source acceptance.

An input reader MAY accept CRLF as transport input only by replacing each CRLF
with one LF before UTF-8/NFC validation and identity calculation. A bare CR is
`E1003_BARE_CR`. The source object recorded in a repository and every cache
key use the resulting normalized LF/NFC bytes. A BOM is
`E1002_BOM_FORBIDDEN`; invalid UTF-8 is `E1001_INVALID_UTF8`; a non-NFC input
is `E1004_NOT_NFC`. An implementation MUST report the earliest offending byte.
Malformed UTF-8 is `E1001_INVALID_UTF8` and is rejected before normalization.
The reference frontend's normalization data MUST be reproducible from the
Unicode 17.0.0 UCD baseline; its exact input files, hashes, and generator
identity are provenance inputs, not ambient host state. See ADR-0029.

The canonical repository path is a validated relative slash-separated path.
It has no `.` or `..` segment, no empty segment, no NUL, and no path separator
other than `/`. A module's declared name maps to this path as specified in
`docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md`.

## 2. Lexical rules

Outside literals and line comments, only ASCII space (`U+0020`) and LF are
whitespace. Horizontal tab is `E1010_TAB_OUTSIDE_LITERAL`; other Unicode
whitespace is `E1011_NON_ASCII_WHITESPACE`. This deliberate restriction makes
layout, source maps and review diffs unambiguous. Four spaces are the project
style; indentation has no syntactic meaning.

A line comment starts with `//` and continues through, but excluding, LF.
Block comments and textual macros do not exist in V1. This makes comment
termination and source-span accounting bounded and local. An SPDX line comment
is ordinary comment text to the language.

Identifiers are ASCII and match:

```text
[A-Za-z_][A-Za-z0-9_]*
```

They are case-sensitive. Unicode is permitted in string data and comments but
not identifiers. A source reader reports `E1012_INVALID_IDENTIFIER` at the
first nonmatching byte rather than applying case folding or confusable mapping.

V1 has no contextual keywords. Every identifier-shaped language word belongs to
exactly one class below. Reserved, primitive, predeclared type, and predeclared
value names cannot be shadowed; every other matching identifier is ordinary.
The inventory is deliberately machine-readable and is checked by
`scripts/check-stage2-language-contract.py` against the EBNF terminals.

<!-- stage2-word-inventory:start -->
```text
reserved: as async await bootstrap borrow break cancel capability const continue defer else enum extern false fn for full if import in join let loop match module mut parallel profile pub record resource return spawn true unsafe uses version while
primitive-type: bool i8 i16 i32 i64 u8 u16 u32 u64 size duration string bytes unit
predeclared-type: Option Result Task TaskResult Shared Region DmaRegion Mutex RwLock Channel Event Semaphore Barrier Latch AtomicBool AtomicU32 AtomicU64 ConversionError slice array
atomic-order: Relaxed Acquire Release AcqRel SeqCst
predeclared-value: Some None Ok Err Completed Cancelled
predeclared-function: to_i8 to_i16 to_i32 to_i64 to_u8 to_u16 to_u32 to_u64 wrapping_add wrapping_sub wrapping_mul
special-token: _
```
<!-- stage2-word-inventory:end -->

`nil` is not a V1 keyword, literal, pattern, type, or absence model. An
ordinary identifier spelled `nil` is allowed, subject to normal name resolution;
unbound use receives `E1202_UNKNOWN_VALUE_NAME`. `Option<T>` is the only V1
typed absence model.

## 3. Literals

Integer literals are decimal (`42`), hexadecimal (`0x2a`) or binary (`0b101010`)
digits with optional single underscores between digits. A leading sign is an
operator, not part of a literal. Invalid base digits, a leading/trailing
underscore, or repeated underscores are `E1020_INVALID_INTEGER_LITERAL`.

An integer suffix is one of `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, or
`i64`. A suffix fixes the literal type and range-checks it. Unsuffixed literals
are contextually typed by a fixed-width operand, parameter, binding annotation,
or return annotation; otherwise they are `i32` and range-checked as `i32`.
There is no target-dependent implicit integer type.

Size literals are an integer literal followed without whitespace by `B`, `KiB`,
`MiB`, or `GiB`; their type is `size`. `KiB = 1024`, `MiB = 1024^2`, and
`GiB = 1024^3`. Duration literals similarly use `ns`, `us`, `ms`, `s`, `min`,
or `h` and have type `duration`. Their represented nanoseconds MUST fit `u64`.

Strings use double quotes and contain Unicode scalar values except unescaped
LF, CR and NUL. Valid escapes are `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\xNN`, and
`\u{H...H}` with one to six hexadecimal digits naming a Unicode scalar value.
`\xNN` inserts one byte whose value must form valid UTF-8 in the completed
string. An invalid escape, invalid scalar, unterminated string, or unescaped
line ending reports `E1030_INVALID_STRING`. A `bytes` literal begins `b"` and
permits only ASCII graphic characters, space, and the byte escapes `\\`,
`\"`, `\n`, `\r`, `\t`, `\0`, and `\xNN`; it reports
`E1031_INVALID_BYTES` otherwise.

## 4. Grammar notation and parser behavior

The grammar uses EBNF. `X?`, `X*`, and `X+` mean optional, zero-or-more, and
one-or-more. Literal tokens are quoted. `identifier`, `integer`, `string`,
`bytes`, `size`, and `duration` refer to the lexical tokens above.

The parser is deterministic. At a declaration-level error it synchronizes at
the next top-level `;` or `]`. At a statement-level error it synchronizes at
the next `;` or the closing brace of the current block. At a comma-separated
list error it synchronizes at `,` or the enclosing closer. It MUST emit the
lowest-numbered applicable lexical error first; then the earliest unconsumed
syntax token; then one recovery diagnostic per synchronization region. It MUST
not guess a missing declaration, capability, type, or operator.

## 5. Complete V1 grammar

```ebnf
source          = module_header import_decl* item* EOF ;
module_header   = "module" module_name "version" version
                  "profile" profile ";" ;
module_name     = identifier ( "." identifier )* ;
qualified_name  = module_name ;
version         = integer "." integer ;
profile         = "bootstrap" | "full" ;

import_decl     = "import" module_name ( "as" identifier )? ";"
                | "import" "capability" module_name "." identifier
                  "as" identifier ";" ;

item            = visibility? resource_decl
                | visibility? record_decl
                | visibility? enum_decl
                | visibility? const_decl
                | visibility? function_decl
                | visibility? extern_decl ;
visibility      = "pub" ;
resource_decl   = "resource" "[" resource_limit_list? "]" ;
resource_limit_list = resource_limit ( "," resource_limit )* ","? ;
resource_limit  = identifier ":" literal ;
record_decl     = "record" identifier "[" field_decl_list? "]" ;
field_decl_list = field_decl ( "," field_decl )* ","? ;
field_decl      = visibility? identifier ":" type ;
enum_decl       = "enum" identifier "[" variant_decl_list? "]" ;
variant_decl_list = variant_decl ( "," variant_decl )* ","? ;
variant_decl    = identifier ( "(" type_list? ")" )?
                | identifier "[" field_decl_list? "]" ;
const_decl      = "const" identifier ":" type "=" expression ";" ;
function_decl   = async_marker? "fn" identifier "(" parameter_list? ")"
                  "->" type effects? block ;
async_marker    = "async" ;
parameter_list  = parameter ( "," parameter )* ","? ;
parameter       = borrow_mode? identifier ":" type ;
borrow_mode     = "borrow" ( "mut" )? ;
effects         = "uses" "[" identifier ( "," identifier )* ","? "]" ;
extern_decl     = "extern" "fn" identifier "(" parameter_list? ")"
                  "->" type effects? ";" ;

type            = primitive_type | predeclared_type | named_type | constructed_type
                | array_type | tuple_type | function_type ;
primitive_type  = "bool" | "i8" | "i16" | "i32" | "i64"
                | "u8" | "u16" | "u32" | "u64" | "size" | "duration"
                | "string" | "bytes" | "unit" ;
predeclared_type = "Event" | "Semaphore" | "Barrier" | "Latch"
                | "AtomicBool" | "AtomicU32" | "AtomicU64"
                | "ConversionError" ;
named_type      = qualified_name ;
constructed_type = "Option" "<" type ">"
                | "Result" "<" type "," type ">"
                | "Task" "<" type ">"
                | "TaskResult" "<" type ">"
                | "Shared" "<" type ">"
                | "Region" "<" type ">"
                | "DmaRegion" "<" type ">"
                | "Mutex" "<" type ">"
                | "RwLock" "<" type ">"
                | "Channel" "<" type ">"
                | "slice" "<" type ">" ;
array_type      = "array" "<" type "," const_expression ">" ;
tuple_type      = "(" type "," type ( "," type )* ","? ")" ;
function_type   = "fn" "(" type_list? ")" "->" type ;
type_list       = type ( "," type )* ","? ;

block           = "{" statement* "}" ;
statement       = let_stmt | assignment ";" | expression ";" | return_stmt
                | break_stmt | continue_stmt | if_stmt | match_stmt
                | while_stmt | for_stmt | loop_stmt | parallel_stmt
                | cancel_stmt | defer_stmt | unsafe_stmt ;
let_stmt        = "let" "mut"? pattern ( ":" type )? "=" expression ";" ;
assignment      = place "=" expression ;
return_stmt     = "return" expression? ";" ;
break_stmt      = "break" ";" ;
continue_stmt   = "continue" ";" ;
if_stmt         = "if" "(" expression ")" block
                ( "else" ( if_stmt | block ) )? ;
match_stmt      = "match" "(" expression ")" "{" match_branch* "}" ;
match_branch    = pattern "=>" block ;
while_stmt      = "while" "(" expression ")" block ;
for_stmt        = "for" pattern "in" "(" expression ")" block ;
loop_stmt       = "loop" block ;
parallel_stmt   = "parallel" block ;
cancel_stmt     = "cancel" expression ";" ;
defer_stmt      = "defer" block ;
unsafe_stmt     = "unsafe" block ;

pattern         = "_" | pattern_name | pattern_name "(" pattern_list? ")"
                | "(" pattern_list ")" ;
pattern_name    = identifier | predeclared_value ;
pattern_list    = pattern ( "," pattern )* ","? ;
expression      = logical_or ;
logical_or      = logical_and ( "||" logical_and )* ;
logical_and     = equality ( "&&" equality )* ;
equality        = comparison ( ( "==" | "!=" ) comparison )* ;
comparison      = bit_or ( ( "<" | "<=" | ">" | ">=" ) bit_or )* ;
bit_or          = bit_xor ( "|" bit_xor )* ;
bit_xor         = bit_and ( "^" bit_and )* ;
bit_and         = shift ( "&" shift )* ;
shift           = sum ( ( "<<" | ">>" ) sum )* ;
sum             = product ( ( "+" | "-" ) product )* ;
product         = unary ( ( "*" | "/" | "%" ) unary )* ;
unary           = ( "!" | "-" | "~" | "borrow" ( "mut" )? | "await" | "join" ) unary
                | postfix ;
postfix         = primary ( call_suffix | index | field | question | cast )* ;
call_suffix     = "(" call_arguments? ")" ;
call_arguments  = positional_argument_list | named_argument_list ;
positional_argument_list = expression ( "," expression )* ","? ;
named_argument_list = named_argument ( "," named_argument )* ","? ;
named_argument  = identifier ":" expression ;
index           = "[" expression "]" ;
field           = "." identifier ;
question        = "?" ;
cast            = "as" type ;
primary         = literal | "true" | "false" | predeclared_value
                | predeclared_function | qualified_name | tuple | array
                | closure | spawn_expression | "(" expression ")" ;
predeclared_value = "Some" | "None" | "Ok" | "Err" | "Completed" | "Cancelled" ;
predeclared_function = "to_i8" | "to_i16" | "to_i32" | "to_i64"
                | "to_u8" | "to_u16" | "to_u32" | "to_u64"
                | "wrapping_add" | "wrapping_sub" | "wrapping_mul" ;
literal         = integer | size | duration | string | bytes ;
tuple           = "(" expression "," expression ( "," expression )* ","? ")" ;
array           = "[" positional_argument_list? "]" ;
closure         = "fn" "(" closure_parameters? ")" block ;
closure_parameters = parameter ( "," parameter )* ","? ;
spawn_expression = "spawn" ( "async" | "parallel" ) block ;
place           = identifier ( field | index )* ;
const_expression = const_sum ;
const_sum       = const_product ( ( "+" | "-" ) const_product )* ;
const_product   = const_primary ( ( "*" | "/" | "%" ) const_primary )* ;
const_primary   = integer | size | identifier | "(" const_expression ")" ;
```

The surface punctuation has one human-facing rule: `()` groups expressions and
contains parameters or call/constructor arguments; `[]` contains declarative
or data lists; `{}` contains executable statements; commas separate list
members; and semicolons terminate simple executable statements. A trailing
comma is permitted in every comma-separated V1 list. A compound statement that
ends in its own `}` takes no following semicolon.

Every control header has mandatory parentheses. The closing `)` therefore ends
an `if`, `while`, `for`, or `match` head before the following executable block
begins; `if ready { ... }` is `E1105_CONTROL_HEAD_PARENS_REQUIRED`. `if` and
`match` are statement-only in V1. Their branches are executable blocks, their
branches have no comma separators, and neither construct is an expression or
an implicit value producer. `while`, `for`, `loop`, and `parallel` are likewise
statement-only; `break` has no value.

A name followed by a call suffix is always one unresolved Call/Construct syntax
node, whether resolution later finds a function, an `Option`/`Result`
constructor, a user enum tuple variant, or a nominal record constructor. The
parser never chooses a constructor parse instead of a function-call parse.
Resolution validates the selected callee kind after that one syntax form is
built; this is not semantic backtracking. Call arguments are either all
positional or all named; the first argument's `identifier ":"` form fixes named
mode. Named arguments are accepted only for nominal record constructors and
named-field enum variants, not ordinary functions or tuple enum variants. They
name every declared field exactly once; an unknown name is
`E1207_UNKNOWN_RECORD_FIELD`, a duplicate is `E1205_DUPLICATE_RECORD_FIELD`,
and an omitted field is
`E1206_MISSING_RECORD_FIELD`. Named argument expressions are evaluated in
source order. `Point(x: 1i32, y: 2i32)` is therefore a record construction;
`Rgb(red: 1u8, green: 2u8, blue: 3u8)` similarly constructs a named-field enum
variant;
`Point { x: 1i32, y: 2i32 }` is not V1 syntax. Missing a comma between list
members is `E1106_LIST_SEPARATOR_REQUIRED`.

Function calls, constructor calls, field access, indexing, propagation (`?`)
and casts group left-to-right; binary precedence is listed from weakest to
strongest. `&&` and `||` short-circuit. `await`, `join`, and `borrow` bind like
other unary operators. A closure and a spawned task use an executable block;
their normal produced value, if any, uses an explicit `return` in that block.
An anonymous closure is `fn (parameters) { ... }`; it uses ordinary typed
parameters in `()` and has an inferred result under docs/40. A plain `{ ... }`
is never an expression and cannot follow `=` or occur as a call argument.

`defer`, `unsafe`, closures, `async`, and `spawn async` are Full-profile
constructs. `parallel`, `spawn parallel`, `join`, and `cancel` have defined
serialized Bootstrap semantics in `docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md`.
An `extern` declaration is reserved by the grammar but rejected as
`E1801_FFI_NOT_AVAILABLE` until a later accepted FFI contract supplies an
interface identifier and capability rule.

## 6. Deliberate exclusions

V1 has no textual macros, implicit imports, wildcard imports, inheritance,
user-defined generic declarations, traits, reflection, exceptions used for
ordinary errors, implicit numeric widening, pointer literals, address casts,
or syntax whose meaning depends on indentation. These exclusions reduce
bootstrap parser and verifier complexity; a later version requires explicit
version negotiation rather than silently reinterpreting V1 source.
