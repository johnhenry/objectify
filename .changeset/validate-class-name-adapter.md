---
"@johnhenry/objectify": patch
---

Security: validate class names in the TypeScript adapter. `Objectify.create({ class })` and class-file lookup (`findClassFile` / `ObjectRef.call`) now reject any class name that is not composed solely of ASCII letters, digits, `_` and `-`, so a name such as `../../tmp/x` can no longer resolve to and execute a `.ts`/`.py` file outside the classes directory. This brings the adapter in line with the Rust CLI, which has validated class names since the fix for #7.
