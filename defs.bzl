"""
Public API for the rinx Bazel rules.

Load this file to access rinx_library, rinx_site and
rinx_inventory.

Example:
    load("@rinx//:defs.bzl", "rinx_library", "rinx_site")
"""

load("//rules:doctest.bzl", _rinx_doctest_tests = "rinx_doctest_tests")
load("//rules:inventory.bzl", _rinx_inventory = "rinx_inventory")
load("//rules:library.bzl", _rinx_library = "rinx_library")
load("//rules:site.bzl", _rinx_site = "rinx_site")

rinx_library = _rinx_library
rinx_site = _rinx_site
rinx_doctest_tests = _rinx_doctest_tests
rinx_inventory = _rinx_inventory
