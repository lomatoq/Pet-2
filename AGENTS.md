# Delivery for this project

The user runs the installed macOS applications, not the source tree. After changing
application code, build and validate the macOS package, then update both
`/Applications/Pet2.app` and `/Applications/Pet2 Dev Console.app` before calling the
work complete. A code commit or ZIP alone is not a completed delivery.

Preserve the pet's saved state and existing bundle identifiers. Keep a recoverable
backup of the previous apps, verify the installed bundles against the package,
and launch the updated Pet when delivering an application update. Do not leave
extra unpacked app copies in the project or `dist`.

Installing these updates is already authorized by the user; do not ask for the
same approval again. Report any actual permission or installation failure clearly.
