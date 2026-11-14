# Unicode grapheme conformance data

`GraphemeBreakTest.txt` is the complete, unmodified Unicode 17.0.0 default
grapheme-cluster break test file: 766 test vectors and 796 physical lines.

Source: [Unicode Character Database 17.0.0](https://www.unicode.org/Public/17.0.0/ucd/auxiliary/GraphemeBreakTest.txt),
retrieved through the [Unicode Consortium's unicodetools mirror](https://github.com/unicode-org/unicodetools/blob/main/unicodetools/data/ucd/17.0.0/auxiliary/GraphemeBreakTest.txt).

SHA-256: `e2d134d2c52919bace503ebb6a551c1855fe1a1faec18478c78fff254a1793ec`.

The conformance test checks every vector both as one text run and split into one
run per Unicode scalar. It verifies extended-grapheme cursor boundaries and
UTF-8/UTF-16 round trips at each boundary. The test asserts the library's Unicode
version matches this fixture; a Unicode upgrade requires a deliberate fixture
update and review. The upstream file header and comments are preserved.
The repository's `.gitattributes` preserves this file's original bytes and excludes
its upstream whitespace from Git whitespace checks.

The following upstream notice applies to this third-party data. It does not
select a software license for Office Engine, whose licensing remains pending.
See also [Unicode's terms of use](https://www.unicode.org/terms_of_use.html) and
the [upstream license](https://github.com/unicode-org/unicodetools/blob/main/LICENSE).

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2001-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0
```
