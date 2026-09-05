"""A tiny module, shown by ``includes.rst`` via ``.. literalinclude::``."""

import sys


# [greet]
def greet(name: str) -> str:
    """Return a greeting for ``name``."""
    return f"Hello, {name}!"
# [/greet]


def main() -> int:
    print(greet("world"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
