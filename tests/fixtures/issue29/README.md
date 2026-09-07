Verbatim reproduction from issue #29: a config setting every rule to
`"error"` next to a file that violates two of them. Both diagnostics
reported as warnings and `check` exited 0, so CI could never fail.

Kept byte-for-byte as reported (`bad_test.gd` is deliberately not
formatted) so `issue_29_full_user_config` reproduces the original
line numbers.
