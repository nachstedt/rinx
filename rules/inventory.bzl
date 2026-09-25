"""
rinx_inventory rule.

Declares another documentation site's `objects.inv` as a pinned input a
rinx_site links into — Sphinx's `intersphinx_mapping`, minus the
fetch. Sphinx downloads an inventory while building; a sandboxed action may
not, so the file is an ordinary label instead: vendored, fetched by the
module system with a checksum (`http_file` with `sha256`), or another
rinx_site's `inventory` output group. Updating the pin is then an
explicit change to the build rather than something that happens to it.
"""

load("//:providers.bzl", "RinxInventoryInfo")

def _rinx_inventory_impl(ctx):
    name = ctx.attr.inventory_name or ctx.label.name
    return [
        RinxInventoryInfo(
            name = name,
            base_url = ctx.attr.base_url,
            file = ctx.file.src,
        ),
        DefaultInfo(files = depset([ctx.file.src])),
    ]

rinx_inventory = rule(
    implementation = _rinx_inventory_impl,
    attrs = {
        "src": attr.label(
            allow_single_file = True,
            mandatory = True,
            doc = "The objects.inv file: a vendored copy, an `http_file` pinned by `sha256`, or a filegroup over another rinx_site's `inventory` output group.",
        ),
        "base_url": attr.string(
            mandatory = True,
            doc = "Where the inventory's pages are published. An absolute URL (`https://docs.python.org/3/`), or a path relative to the consuming site's root (`../api/`) for a site deployed next to it — each page's link is then made relative to that page.",
        ),
        "inventory_name": attr.string(
            doc = "The name documents pick this inventory out by, as in :external+python:ref:`tut` or :ref:`python:tut`. Defaults to the target's name.",
        ),
    },
    doc = """
Declares a Sphinx inventory (`objects.inv`) a rinx_site may link into.

List it in the site's `inventories`; the site's index action reads it, and a
reference no document of the site defines is then resolved against it.
""",
)
