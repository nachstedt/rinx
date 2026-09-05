Block Quotes
=============

A block quote is text indented relative to its surrounding paragraph, with no
directive or list marker introducing it — indentation alone is the markup.

A Plain Block Quote
---------------------

Sherlock Holmes was, I take it, the most perfect reasoning and observing
machine that the world has seen.

    All emotions, and that one particularly, were abhorrent to his cold,
    precise but admirably balanced mind.

With a Single-Line Attribution
----------------------------------

    It is my business to know things. That is my trade.

    -- Sherlock Holmes

With a Multi-Line Attribution
---------------------------------

    A long habit of not thinking a thing wrong gives it a superficial
    appearance of being right.

    -- Thomas Paine,
       Common Sense

Nested Block Quotes
----------------------

    A block quote may itself contain a further-indented block quote.

        The world is full of obvious things which nobody by any chance
        ever observes.

        -- Sherlock Holmes

A Block Quote Containing a List
-----------------------------------

Indenting a list without any enclosing directive nests it inside a block
quote, exactly as the RST grammar requires:

    - Elementary.
    - The game is afoot.

A Block Quote Containing a Directive
----------------------------------------

The same is true of a directive: indentation wins over the directive marker,
so this note ends up nested inside the block quote around it rather than
rendered in place.

    .. note::

       A note, quoted.
