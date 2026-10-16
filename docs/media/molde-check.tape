# Records docs/media/molde-check.gif for the README: `vhs docs/media/molde-check.tape`
# from the repository root, after `cargo build --release`. Needs vhs and jq.
Output docs/media/molde-check.gif
Set Shell "bash"
Set FontSize 16
Set Width 1000
Set Height 600
Set Padding 20
Set TypingSpeed 40ms
Set Theme "Builtin Dark"

Hide
Type "export PATH=$PWD/target/release:$PATH PS1='$ ' && W=$(mktemp -d) && cp -R examples/demo $W/site && cd $W && clear"
Enter
Show

Type "molde check --site site --out out | jq -c '{verdict, published}'"
Enter
Sleep 1500ms
Type "# an agent adds a link to the home page, with a typo"
Enter
Sleep 400ms
Type `sed -i.bak 's|<h1>The library</h1>|&<p><a href="/books/dnue/">Start with Dune</a></p>|' site/templates/index.html.j2`
Enter
Sleep 400ms
Type "molde check --site site --out out | jq '{verdict, published, errors: [.errors[] | {check, page, broken_href, template, hint}]}'"
Enter
Sleep 4s
Type "# nothing was published: the last good build stays live. The agent fixes it:"
Enter
Sleep 400ms
Type "sed -i.bak 's|dnue|dune|' site/templates/index.html.j2"
Enter
Sleep 400ms
Type "molde check --site site --out out | jq -c '{verdict, published}'"
Enter
Sleep 4s
