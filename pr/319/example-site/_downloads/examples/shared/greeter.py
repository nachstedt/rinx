"""A tiny module, shown by ``includes.rst`` via ``.. literalinclude::``."""

import sys


# [greet]
def greet(name: str, times: int = 1) -> str:
    """Return a greeting for ``name``, repeated ``times`` times."""
    line = f"Hello, {name}!"
    return "\n".join([line] * times)
# [/greet]


def main() -> int:
    print(greet("world"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
