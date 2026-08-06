<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Extensible language frontends

## Objective

TOS should learn new programming languages without enlarging the binary nucleus. A language frontend is a textual system module that parses source in another language, performs semantic analysis, and emits verified TOS IR.

## Frontend contract

A frontend provides versioned functions equivalent to:

```tos
interface LanguageFrontendV1 {
    fn describe() -> LanguageDescriptor
    fn probe(path: string, prefix: bytes) -> ProbeResult
    fn parse(source: SourceUnit, options: ParseOptions) -> Result<SyntaxUnit, Diagnostics>
    fn analyze(unit: SyntaxUnit, imports: ImportResolver) -> Result<TypedUnit, Diagnostics>
    fn lower(unit: TypedUnit, target: IrTarget) -> Result<IrModule, Diagnostics>
    fn format(source: SourceUnit, options: FormatOptions) -> Result<string, Diagnostics>
}
```

The actual ABI is defined in schemas, not by relying on textual syntax shown here.

## Language descriptor

A descriptor declares:

- language name and stable identifier;
- frontend source content ID;
- supported language versions;
- file extensions and optional shebang forms;
- required runtime services;
- whether bootstrap use is supported;
- deterministic behavior guarantees;
- sandbox limits;
- compatibility claims and known deviations.

## Installation

A frontend is installed as source under a system path such as:

```text
/system/languages/lua/
/system/languages/scheme/
/system/languages/python-subset/
```

Its manifest is part of the system commit. Activating a frontend follows the same candidate validation and rollback rules as other system modules.

## Trust and isolation

A frontend processes untrusted text and therefore runs in a restricted process. It does not automatically receive filesystem, network, device, or repository-write capabilities.

A malicious or defective frontend can fail compilation of its language but should not compromise the nucleus or other modules.

## Import resolution

Frontends do not fetch dependencies directly. They request imports through a deterministic resolver bound to:

- the selected system commit;
- explicit package or module commits;
- the current working overlay when allowed;
- declared lock data.

Network resolution is a separate explicit system operation.

## Compatibility levels

Each frontend states one of:

- `native` — language designed for TOS and fully specified by the project;
- `compatible` — aims to conform to an external language specification;
- `subset` — intentionally supports a named subset;
- `translated` — accepts source but maps semantics through documented changes;
- `syntax-only` — tooling support without execution.

TOS never labels a subset as full compatibility.

## Foreign runtimes

Some languages require a runtime, garbage collector, dynamic object model, or native extension system. Those components run as textual services or verified derived caches where possible.

Native extensions from conventional ecosystems are not silently accepted. They require an explicit compatibility process, sandbox boundary, or source port.

## Bootstrapping new frontends

The first frontend is TOS Core and is implemented partly in the nucleus and partly as boot modules. Later frontends should be written in TOS Core. Once the system is self-hosting, portions of the TOS Core frontend may also move out of the nucleus, provided recovery retains an independently bootable reference implementation.

## Architectural limits on frontends

A frontend teaches TOS to understand another textual language. It does not receive ambient hardware access, define a second package universe or replace system commit identity.

A frontend descriptor declares:

- source media type and normalization rules;
- semantic compatibility profile;
- required frontend capabilities;
- emitted IR version range;
- deterministic dependency resolution rules;
- cache and source-map behavior;
- licence metadata for runtime components.

A language syntax subset must be labelled as a subset. Calling an external interpreter through IPC is a foreign runtime integration, not native frontend compatibility.
