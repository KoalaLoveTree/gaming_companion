# How we work

Product behavior stays in `SPEC.md`. This file is only the working shape. Revisit it after a few days of use, or when a session feels wasteful.

## One slice

Say what this change is, and what it leaves out. A short plan names the files, how the change works, and the edges. Then stop, unless the go-ahead for that slice was already given.

The patch stays small enough to review. Run it. The next slice comes from using it.

A settled slice can include its sentence in `SPEC.md`, the code, and the test. A new shape is discussed before it is coded.

When a change is a large page script on both platforms, do the platform we can run first.

## Leave out

No ticket board, and no rule that work may start only from a "Now" item. A side idea gets one mention. It is not part of the current patch.

## Where facts live

`SPEC.md` is the product. Workspace memory keeps this machine, build commands, and bugs already paid for. Do not copy `SPEC.md` into memory or into this file.
