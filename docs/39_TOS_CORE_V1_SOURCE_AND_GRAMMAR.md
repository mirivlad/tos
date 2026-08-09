<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 — source model and grammar

- Status: **Proposed Stage 2 contract — not implementation authority**
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

This document is the proposed lexical and syntactic part of one TOS Core V1
contract set. It is intentionally detailed enough to prevent a first parser
from inventing language semantics. It becomes Tier 2 authority only if the
Project Architect accepts ADR-0028. Until then it is a reviewable proposal and
does **not** authorize a production parser, checker, IR, verifier, interpreter,
cache, or runtime.

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

An input reader MAY accept CRLF as transport input only by replacing each CRLF
with one LF before UTF-8/NFC validation and identity calculation. A bare CR is
`E1003_BARE_CR`. The source object recorded in a repository and every cache
key use the resulting normalized LF/NFC bytes. A BOM is
`E1002_BOM_FORBIDDEN`; invalid UTF-8 is `E1001_INVALID_UTF8`; a non-NFC input
is `E1004_NOT_NFC`. An implementation MUST report the earliest offending byte.

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

The reserved words are:

```text
as async atomic await bootstrap bool borrow break cancel capability const continue
defer else enum error extern false for fn from if import in let loop match
full module mut nil parallel profile pub record requires resource return self
spawn string task true type unsafe use uses while
```

`Option`, `Result`, `Task`, `Shared`, `Region`, `DmaRegion`, `Mutex`,
`RwLock`, `Channel`, `Event`, `Semaphore`, `Barrier`, `Latch`, `AtomicBool`, `AtomicU32`,
and `AtomicU64` are predeclared type names, not keywords. A program cannot
shadow a reserved word or a predeclared type name. `Relaxed`, `Acquire`,
`Release`, `AcqRel`, and `SeqCst` are predeclared atomic-order values and also
cannot be shadowed.

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
the next top-level `;` or `}`. At a statement-level error it synchronizes at
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
resource_decl   = "resource" "{" resource_limit* "}" ;
resource_limit  = identifier ":" literal ";" ;
record_decl     = "record" identifier "{" field_decl* "}" ;
field_decl      = visibility? identifier ":" type ";" ;
enum_decl       = "enum" identifier "{" variant_decl ( "," variant_decl )*
                  ","? "}" ;
variant_decl    = identifier ( "(" type_list? ")" )?
                | identifier "{" field_decl* "}" ;
const_decl      = "const" identifier ":" type "=" expression ";" ;
function_decl   = async_marker? "fn" identifier "(" parameter_list? ")"
                  "->" type effects? block ;
async_marker    = "async" ;
parameter_list  = parameter ( "," parameter )* ","? ;
parameter       = borrow_mode? identifier ":" type ;
borrow_mode     = "borrow" ( "mut" )? ;
effects         = "uses" "{" identifier ( "," identifier )* ","? "}" ;
extern_decl     = "extern" "fn" identifier "(" parameter_list? ")"
                  "->" type effects? ";" ;

type            = primitive_type | named_type | constructed_type
                | array_type | function_type ;
primitive_type  = "bool" | "i8" | "i16" | "i32" | "i64"
                | "u8" | "u16" | "u32" | "u64" | "size" | "duration"
                | "string" | "bytes" | "unit" ;
named_type      = qualified_name ;
constructed_type = ( "Option" | "Result" | "Task" | "Shared" | "Region"
                   | "DmaRegion" | "Mutex" | "RwLock" | "Channel" | "Semaphore" )
                   "<" type_list ">" ;
array_type      = "[" type ";" const_expression "]" ;
function_type   = "fn" "(" type_list? ")" "->" type ;
type_list       = type ( "," type )* ","? ;

block           = "{" statement* tail_expression? "}" ;
tail_expression = expression ;
statement       = let_stmt | assignment ";" | expression ";" | return_stmt
                | break_stmt | continue_stmt | if_stmt | while_stmt | for_stmt
                | loop_stmt | match_stmt | parallel_stmt | cancel_stmt
                | defer_stmt | unsafe_stmt ;
let_stmt        = "let" "mut"? pattern ( ":" type )? "=" expression ";" ;
assignment      = place "=" expression ;
return_stmt     = "return" expression? ";" ;
break_stmt      = "break" expression? ";" ;
continue_stmt   = "continue" ";" ;
if_stmt         = "if" expression block ( "else" ( if_stmt | block ) )? ;
while_stmt      = "while" expression block ;
for_stmt        = "for" pattern "in" expression block ;
loop_stmt       = "loop" block ;
match_stmt      = "match" expression "{" match_arm* "}" ;
match_arm       = pattern "=>" ( block | expression "," ) ;
parallel_stmt   = "parallel" block ;
cancel_stmt     = "cancel" expression ";" ;
defer_stmt      = "defer" block ;
unsafe_stmt     = "unsafe" block ;

pattern         = "_" | identifier | "nil" | identifier "(" pattern_list? ")"
                | "(" pattern_list ")" ;
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
postfix         = primary ( call | index | field | question | cast )* ;
call            = "(" argument_list? ")" ;
argument_list   = expression ( "," expression )* ","? ;
index           = "[" expression "]" ;
field           = "." identifier ;
question        = "?" ;
cast            = "as" type ;
primary         = literal | "true" | "false" | "nil" | qualified_name
                | tuple | array | record_init | enum_init
                | closure | spawn_expression | "(" expression ")" | block ;
literal         = integer | size | duration | string | bytes ;
tuple           = "(" expression "," expression ( "," expression )* ","? ")" ;
array           = "[" argument_list? "]" ;
record_init     = qualified_name "{" field_init* "}" ;
field_init      = identifier ":" expression ","? ;
enum_init       = qualified_name "(" argument_list? ")" ;
closure         = "|" closure_parameters? "|" expression ;
closure_parameters = parameter ( "," parameter )* ","? ;
spawn_expression = "spawn" ( "async" | "parallel" ) block ;
place           = identifier ( field | index )* ;
const_expression = const_sum ;
const_sum       = const_product ( ( "+" | "-" ) const_product )* ;
const_product   = const_primary ( ( "*" | "/" | "%" ) const_primary )* ;
const_primary   = integer | size | identifier | "(" const_expression ")" ;
```

`record_init` and `enum_init` are resolved only after parsing: a name followed
by `{` or `(` is syntactically accepted, then type resolution decides whether
it denotes a record or variant. This is a local deterministic disambiguation,
not semantic backtracking. Function calls, field access, indexing, propagation
(`?`) and casts group left-to-right; binary precedence is listed from weakest
to strongest. `&&` and `||` short-circuit. `await`, `join`, and `borrow` bind
like other unary operators.

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
