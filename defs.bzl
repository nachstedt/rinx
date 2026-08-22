"""
Public API for the rusty_sphinx Bazel rules.

Load this file to access rusty_sphinx_library and rusty_sphinx_site.

Example:
    load("@rusty_sphinx//:defs.bzl", "rusty_sphinx_library", "rusty_sphinx_site")
"""

load("//rules:doctest.bzl", _rusty_sphinx_doctest_tests = "rusty_sphinx_doctest_tests")
load("//rules:library.bzl", _rusty_sphinx_library = "rusty_sphinx_library")
load("//rules:site.bzl", _rusty_sphinx_site = "rusty_sphinx_site")

rusty_sphinx_library = _rusty_sphinx_library
rusty_sphinx_site = _rusty_sphinx_site
rusty_sphinx_doctest_tests = _rusty_sphinx_doctest_tests
