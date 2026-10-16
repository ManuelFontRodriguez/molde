<?php
// Reference strings for tests/cldr.rs, printed by PHP's intl (ICU4C) — the formatter
// WordPress themes use. Regenerate after a CLDR bump, with an ICU
// whose CLDR matches tools/cldr/generate.py VERSION:
//
//   docker run --rm --user root -v "$PWD":/w --entrypoint /bin/sh wordpress:cli-php8.2 \
//     -c 'apk add -q icu-data-full && php /w/tools/cldr/goldens.php' > tests/cldr/icu.json
//
// (the Alpine image ships English-only ICU data until icu-data-full is added.)
// Numbers go in as PHP floats, as the theme passes them; every case is exactly
// representable enough that ICU's shortest round-trip digits equal the string molde gets.

$locales = [
    // 22 widely used locales
    "en", "es", "pt", "nl", "de", "fr", "it", "ru", "ja", "tr", "ko", "pl", "el", "sv", "no",
    "da", "fi", "he", "zh", "hi", "uk", "ar",
    // resolution and numbering-system edges
    "nb", "nn", "pt_PT", "en_GB", "en_IN", "de_CH", "es_MX", "fr_CA", "zh_TW", "zh_HK", "sr",
    "sr_ME", "ar_EG", "fa", "bn", "th", "iw",
];
$tz = new DateTimeZone("UTC");

function num($l, $v, $decimals, $grouping = true) {
    $f = new NumberFormatter($l, NumberFormatter::DECIMAL);
    $f->setAttribute(NumberFormatter::MIN_FRACTION_DIGITS, $decimals);
    $f->setAttribute(NumberFormatter::MAX_FRACTION_DIGITS, $decimals);
    if (!$grouping) {
        $f->setAttribute(NumberFormatter::GROUPING_USED, 0);
    }
    return $f->format($v);
}

function pattern($l, $tz, $pattern, $date) {
    $f = new IntlDateFormatter($l, IntlDateFormatter::NONE, IntlDateFormatter::NONE, $tz,
        IntlDateFormatter::GREGORIAN, $pattern);
    return $f->format(new DateTime($date, $tz));
}

// [value, decimals, grouping]; decimals null = molde's verbatim mode, compared with ICU
// at the value's own fraction digits and no grouping
$numbers = [
    ["1234.5", 2, true], ["0.5", null, false], ["-1234.5", 2, true], ["-0.0850", null, false],
    ["1234.5", 1, false], ["12345.678", 0, true], ["1234567.891", 2, true], ["0.125", 2, true],
    ["0.135", 2, true], ["2.675", 2, true], ["-0.001", 2, true], ["1234", 0, true],
    ["999.995", 2, true], ["1234567.5", null, false],
];
$styles = ["full" => IntlDateFormatter::FULL, "long" => IntlDateFormatter::LONG,
    "medium" => IntlDateFormatter::MEDIUM, "short" => IntlDateFormatter::SHORT];

$out = ["icu" => INTL_ICU_VERSION, "icu_data" => INTL_ICU_DATA_VERSION, "php" => PHP_VERSION,
    "numbers" => [], "dates" => [], "patterns" => []];
foreach ($locales as $l) {
    $tag = str_replace("_", "-", $l);
    foreach ($numbers as [$v, $d, $g]) {
        $digits = $d ?? (strpos($v, ".") === false ? 0 : strlen($v) - strpos($v, ".") - 1);
        $out["numbers"][] = [$tag, $v, $d, $g, num($l, (float)$v, $digits, $g)];
    }
    foreach (["2026-03-05", "1999-12-31", "2026-11-03", "2026-09-18"] as $date) {
        foreach ($styles as $name => $style) {
            $f = new IntlDateFormatter($l, $style, IntlDateFormatter::NONE, $tz, IntlDateFormatter::GREGORIAN);
            $out["dates"][] = [$tag, $date, $name, $f->format(new DateTime($date, $tz))];
        }
        $p = (new IntlDatePatternGenerator($l))->getBestPattern("yMMMM");
        $out["dates"][] = [$tag, $date, "month", pattern($l, $tz, $p, $date)];
    }
    foreach (["MMMM", "LLLL", "EEEE, dd MMMM yyyy", "EEE d MMM yy", "d/M/y G"] as $p) {
        $out["patterns"][] = [$tag, "2026-03-05", $p, pattern($l, $tz, $p, "2026-03-05")];
    }
}
// one case per line, so a CLDR bump reviews as a readable diff
$j = fn($v) => json_encode($v, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES);
echo "{\n";
foreach (["icu", "icu_data", "php"] as $k) {
    echo "  ", $j($k), ": ", $j($out[$k]), ",\n";
}
$sections = ["numbers", "dates", "patterns"];
foreach ($sections as $i => $k) {
    echo "  ", $j($k), ": [\n";
    $rows = array_map(fn($r) => "    " . $j($r), $out[$k]);
    echo implode(",\n", $rows), "\n  ]", $i + 1 < count($sections) ? "," : "", "\n";
}
echo "}\n";
