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
open Apero
open Ztypes

type t = {
  has_header : bool; 
  buffer : Abuf.t;
}

module PayloadHeaderFlags = struct
  let srcidFlag =    char_of_int 0x01
  let srcsnFlag =    char_of_int 0x02
  let bkridFlag =    char_of_int 0x04
  let bkrsnFlag =    char_of_int 0x08
  let tsFlag =       char_of_int 0x10
  let kindFlag =     char_of_int 0x20
  let encodingFlag = char_of_int 0x40

  let hasFlag h f =  (int_of_char h) land (int_of_char f) <> 0
  let flags l = 
    let rec intflags = function 
    | [] -> 0x00 
    | h :: t -> (int_of_char h) lor (intflags t)
    in char_of_int @@ intflags l
end 

let decode_payload_header buf = 
  let open PayloadHeaderFlags in
  Abuf.read_byte buf
  |> fun c -> 

  (match hasFlag c srcidFlag with 
  | false -> None
  | true -> Some(decode_vle buf |> Vle.to_int |> fun l -> Abuf.read_buf l buf))
  |> fun srcid -> 

  (match hasFlag c srcsnFlag with 
  | false -> None
  | true -> Some(decode_vle buf))
  |> fun srcsn -> 

  (match hasFlag c bkridFlag with 
  | false -> None
  | true -> Some(decode_vle buf |> Vle.to_int |> fun l -> Abuf.read_buf l buf))
  |> fun bkrid -> 

  (match hasFlag c bkrsnFlag with 
  | false -> None
  | true -> Some(decode_vle buf))
  |> fun bkrsn -> 

  (match hasFlag c tsFlag with 
  | false -> None
  | true -> Some(Timestamp.decode buf))
  |> fun ts -> 

  (match hasFlag c kindFlag with 
  | false -> None
  | true -> Some(decode_vle buf))
  |> fun kind -> 

  (match hasFlag c encodingFlag with 
  | false -> None
  | true -> Some(decode_vle buf))
  |> fun encoding -> 

  {srcid; srcsn; bkrid; bkrsn; ts; kind; encoding}

let skip_payload_header buf = 
  let open PayloadHeaderFlags in
  Abuf.read_byte buf
  |> fun c -> 

  (match hasFlag c srcidFlag with 
  | false -> ()
  | true -> decode_vle buf |> Vle.to_int |> fun l -> Abuf.skip l buf);

  (match hasFlag c srcsnFlag with 
  | false -> ()
  | true -> skip_vle buf);

  (match hasFlag c bkridFlag with 
  | false -> ()
  | true -> decode_vle buf |> Vle.to_int |> fun l -> Abuf.skip l buf);

  (match hasFlag c bkrsnFlag with 
  | false -> ()
  | true -> skip_vle buf);

  (match hasFlag c tsFlag with 
  | false -> ()
  | true -> Timestamp.decode buf |> ignore);

  (match hasFlag c kindFlag with 
  | false -> ()
  | true -> skip_vle buf);

  (match hasFlag c encodingFlag with 
  | false -> ()
  | true -> skip_vle buf)

let encode_payload_header dh buf = 
  let open PayloadHeaderFlags in
  let f = [] 
  |> (fun f -> match dh.srcid with | Some _ -> srcidFlag :: f | None -> f)
  |> (fun f -> match dh.srcsn with | Some _ -> srcsnFlag :: f | None -> f)
  |> (fun f -> match dh.bkrid with | Some _ -> bkridFlag :: f | None -> f)
  |> (fun f -> match dh.bkrsn with | Some _ -> bkrsnFlag :: f | None -> f)
  |> (fun f -> match dh.ts with | Some _ -> tsFlag :: f | None -> f)
  |> (fun f -> match dh.kind with | Some _ -> kindFlag :: f | None -> f)
  |> (fun f -> match dh.encoding with | Some _ -> encodingFlag :: f | None -> f) in
  
  Abuf.write_byte (PayloadHeaderFlags.flags f) buf;
  (match dh.srcid with 
  | Some wid -> encode_vle (Vle.of_int (Abuf.readable_bytes wid)) buf; Abuf.write_buf wid buf
  | None -> ());
  (match dh.srcsn with 
  | Some wsn -> encode_vle wsn buf
  | None -> ());
  (match dh.bkrid with 
  | Some bkid -> encode_vle (Vle.of_int (Abuf.readable_bytes bkid)) buf; Abuf.write_buf bkid buf
  | None -> ());
  (match dh.bkrsn with 
  | Some wsn -> encode_vle wsn buf
  | None -> ());
  (match dh.ts with 
  | Some ts -> Timestamp.encode ts buf
  | None -> ());
  (match dh.kind with 
  | Some kind -> encode_vle kind buf
  | None -> ());
  (match dh.encoding with 
  | Some code -> encode_vle code buf
  | None -> ())

let create ?header:(h=empty_data_info) d = 
  let hbuf = Abuf.create ~grow:32 32 in 
  encode_payload_header h hbuf; 
  let buffer =  Abuf.wrap [hbuf; d] in 
  {has_header=true;buffer}

let from_buffer h b = {has_header=h; buffer=b}

let header p = 
  p.has_header |> function 
  | true -> Abuf.set_r_pos 0 p.buffer; decode_payload_header p.buffer
  | false -> empty_data_info

let buffer p = 
  p.has_header |> function 
  | true -> Abuf.set_r_pos 0 p.buffer; p.buffer
  | false -> 
    Abuf.set_r_pos 0 p.buffer; 
    let hbuf = Abuf.create 1 in 
    Abuf.write_byte '\x00' hbuf; 
    Abuf.wrap [hbuf; p.buffer]

let data p = 
  p.has_header |> function 
  | true -> Abuf.set_r_pos 0 p.buffer;            
            skip_payload_header p.buffer;
            Abuf.slice (Abuf.r_pos p.buffer) (Abuf.readable_bytes p.buffer) p.buffer
  | false -> Abuf.duplicate p.buffer

let with_timestamp p ts = 
  let header = {(header p) with ts = (Some ts)} in 
  create ~header (data p)