(*
 * Copyright (c) 2017, 2020 ADLINK Technology Inc.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Eclipse Public License 2.0 which is available at
 * http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
 * which is available at https://www.apache.org/licenses/LICENSE-2.0.
 *
 * SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
 *
 * Contributors:
 *   ADLINK zenoh team, <zenoh@adlink-labs.tech>
 *)
open Zenoh_types
open Zenoh_common_errors

let ascii_table = List.init 256 (fun i -> char_of_int i)

let acceptable_chars = List.filter (fun c -> c > ' ' && c != '/' && c != '*' && c != '?' && c != '[' && c != ']' && c != '#') ascii_table


let check_if b ?arg line =
  let test_name =
    match arg with
    | None -> Printf.sprintf "test line %d" line
    | Some(s) -> Printf.sprintf "test line %d with %s" line s
  in
    Alcotest.(check bool) test_name b

let test_matching s p =
  try 
    Selector.is_matching_path (Path.of_string p) (Selector.of_string s) 
  with 
    | YException e -> print_endline @@ show_yerror e; false


let test_selector s path predicate properties fragment =
  try
    let sel = Selector.of_string s in
    Selector.path sel = path &&
    Selector.predicate sel = predicate &&
    Selector.properties sel = properties &&
    Selector.fragment sel = fragment
  with 
    | YException e -> print_endline @@ show_yerror e; false

let test_validity () =
  check_if true  __LINE__ @@ test_selector "/a/b/c" "/a/b/c" None None None;
  check_if true  __LINE__ @@ test_selector "/a b c" "/a b c" None None None;
  check_if true  __LINE__ @@ test_selector "/a b/c" "/a b/c" None None None;
  check_if true  __LINE__ @@ test_selector "/*" "/*" None None None;
  check_if true  __LINE__ @@ test_selector "/*/" "/*" None None None;
  check_if true  __LINE__ @@ test_selector "/a*" "/a*" None None None;
  check_if true  __LINE__ @@ test_selector "/**" "/**" None None None;
  check_if true  __LINE__ @@ test_selector "/**/" "/**" None None None;
  check_if true  __LINE__ @@ test_selector "/a/b?" "/a/b" None None None;
  check_if true  __LINE__ @@ test_selector "/a/b?q" "/a/b" (Some "q") None None;
  check_if true  __LINE__ @@ test_selector "/a/b?q1?q2" "/a/b" (Some "q1?q2") None None;
  check_if true  __LINE__ @@ test_selector "/a/b?q1(p)" "/a/b" (Some "q1") (Some "p") None;
  check_if true  __LINE__ @@ test_selector "/a/b?q1()" "/a/b" (Some "q1") (Some "") None;
  check_if true  __LINE__ @@ test_selector "/a/b?(p)" "/a/b" None (Some "p") None;
  check_if true  __LINE__ @@ test_selector "/a/b?()" "/a/b" None (Some "") None;
  check_if true  __LINE__ @@ test_selector "/a/b?()#" "/a/b" None (Some "") (Some "");
  check_if true  __LINE__ @@ test_selector "/a/b#f" "/a/b" None None (Some "f");
  check_if true  __LINE__ @@ test_selector "/a/b#" "/a/b" None None (Some "");
  check_if true  __LINE__ @@ test_selector "/a/b#f1#f2" "/a/b" None None (Some "f1#f2");
  check_if true  __LINE__ @@ test_selector "/a/b?q#f" "/a/b" (Some "q") None (Some "f");
  check_if true  __LINE__ @@ test_selector "/a/b?q(p)#f" "/a/b" (Some "q") (Some "p") (Some "f");
  check_if true  __LINE__ @@ test_selector "/a/b?q1?q2(p1(1)p2(2))#f1#f2?q3" "/a/b" (Some "q1?q2") (Some "p1(1)p2(2)") (Some "f1#f2?q3");
  check_if true  __LINE__ @@ test_selector "/" "/" None None None;
  check_if true  __LINE__ @@ test_selector "/a/" "/a" None None None;
  check_if true  __LINE__ @@ test_selector "//a/" "/a" None None None;
  check_if true  __LINE__ @@ test_selector "////a///" "/a" None None None;
  check_if true  __LINE__ @@ test_selector "////a//b///c/" "/a/b/c" None None None;
  check_if false __LINE__ @@ test_selector "" "" None None None;
  check_if false __LINE__ @@ test_selector "abc" "" None None None;
  check_if false  __LINE__ @@ test_selector "/a**" "/a**" None None None;
  ()

let test_simple_selectors () =
  check_if true __LINE__ @@ test_matching "/" "/";
  check_if true __LINE__ @@ test_matching "/a" "/a";
  check_if true __LINE__ @@ test_matching "/a/" "/a";
  check_if true __LINE__ @@ test_matching "/a" "/a/";
  check_if true __LINE__ @@ test_matching "/a/b" "/a/b";
  check_if true __LINE__ @@ test_matching "/a/b?q" "/a/b";
  check_if true __LINE__ @@ test_matching "/a/b#f" "/a/b";
  check_if true __LINE__ @@ test_matching "/a/b?q#f" "/a/b";
  List.iter (fun c -> let arg = Printf.sprintf "/%c" c in check_if true __LINE__ ~arg @@ test_matching arg arg) acceptable_chars;
  ()

let test_wildcard_selectors () =
  check_if true  __LINE__ @@ test_matching "/*" "/abc";
  check_if true  __LINE__ @@ test_matching "/*" "/abc/";
  check_if true  __LINE__ @@ test_matching "/*/" "/abc";
  check_if false __LINE__ @@ test_matching "/*" "/";
  check_if false __LINE__ @@ test_matching "/*" "xxx";
  check_if true  __LINE__ @@ test_matching "/ab*" "/abcd";
  check_if true  __LINE__ @@ test_matching "/ab*d" "/abcd";
  check_if true  __LINE__ @@ test_matching "/ab*" "/ab";
  check_if false __LINE__ @@ test_matching "/ab/*" "/ab";
  check_if true  __LINE__ @@ test_matching "/a/*/c/*/e" "/a/b/c/d/e";
  check_if true  __LINE__ @@ test_matching "/a/*b/c/*d/e" "/a/xb/c/xd/e";
  check_if false __LINE__ @@ test_matching "/a/*/c/*/e" "/a/c/e";
  check_if false __LINE__ @@ test_matching "/a/*/c/*/e" "/a/b/c/d/x/e";
  check_if true  __LINE__ @@ test_matching "/a/*/c/*/e?q" "/a/b/c/d/e";
  check_if true  __LINE__ @@ test_matching "/a/*/c/*/e#f" "/a/b/c/d/e";
  check_if true  __LINE__ @@ test_matching "/a/*/c/*/e?q#f" "/a/b/c/d/e";
  check_if false __LINE__ @@ test_matching "/ab*cd" "/abxxcxxd";
  check_if true  __LINE__ @@ test_matching "/ab*cd" "/abxxcxxcd";
  check_if false __LINE__ @@ test_matching "/ab*cd" "/abxxcxxcdx";
  List.iter (fun c -> let arg = Printf.sprintf "/%c" c in check_if true __LINE__ ~arg @@ test_matching "/*" arg) acceptable_chars;
  ()

let test_2_wildcard_selectors () =
  check_if true  __LINE__ @@ test_matching "/**" "/abc";
  check_if true  __LINE__ @@ test_matching "/**" "/a/b/c";
  check_if true  __LINE__ @@ test_matching "/**" "/a/b/c/";
  check_if true  __LINE__ @@ test_matching "/**/" "/a/b/c";
  check_if true  __LINE__ @@ test_matching "/**/" "/";
  check_if true  __LINE__ @@ test_matching "/ab*/**" "/abcd/ef";
  check_if true  __LINE__ @@ test_matching "/ab/**" "/ab";
  check_if true  __LINE__ @@ test_matching "/**/xyz" "/a/b/xyz/d/e/f/xyz";
  check_if true  __LINE__ @@ test_matching "/**/xyz/**/xyz" "/a/b/xyz/d/e/f/xyz";
  check_if true  __LINE__ @@ test_matching "/a/**/c/**/e" "/a/b/b/b/c/d/d/d/e";
  check_if true __LINE__ @@ test_matching "/a/**/c/**/e" "/a/c/e";
  check_if true  __LINE__ @@ test_matching "/a/**/c/**/e?q" "/a/b/b/b/c/d/d/d/e";
  check_if true  __LINE__ @@ test_matching "/a/**/c/**/e#f" "/a/b/b/b/c/d/d/d/e";
  check_if true  __LINE__ @@ test_matching "/a/**/c/**/e?q#f" "/a/b/b/b/c/d/d/d/e";
  check_if true  __LINE__ @@ test_matching "/a/**/c/*/e/*" "/a/b/b/b/c/d/d/c/d/e/f";
  check_if false __LINE__ @@ test_matching "/a/**/c/*/e/*" "/a/b/b/b/c/d/d/c/d/d/e/f";
  List.iter (fun c -> let arg = Printf.sprintf "/%c" c in check_if true __LINE__ ~arg @@ test_matching "/**" arg) acceptable_chars;
  ()

let all_tests = [
  "Selectors validity", `Quick, test_validity;
  "Selectors no *" , `Quick, test_simple_selectors;
  "Selectors with *" , `Quick, test_wildcard_selectors;
  "Selectors with **" , `Quick, test_2_wildcard_selectors;
]
