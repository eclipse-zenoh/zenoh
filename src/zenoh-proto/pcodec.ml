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
(* open Apero
open Apero.Result

let encode_properties ps =
  if ps = Properties.empty then return 
  else (encode_seq encode_property) ps 
  
let decode_properties = (decode_seq decode_property)  *)

(* let encode_properties = Apero.encode_properties *)
  
(* 
let decode_properties h buf =
  let open Message in 
  let open Apero in   
  let open Apero.Result.Infix in

  if Flags.(hasFlag h pFlag) then 
    begin 
      let ps = decode_properties buf in 
      ps >>= fun (ps, buf) -> Ok (Properties.of_list ps, buf)
    end
  else Result.return (Properties.empty, buf) 
 *)



(*   
let encode_properties ps =
  if ps = Properties.empty then return 
  else (encode_seq encode_property) ps 
  
let decode_properties = (decode_seq decode_property) 



*)

let decode_property buf =
  Apero.fast_decode_vle buf 
  |> (fun id -> 
      Apero.decode_buf buf 
      |> (fun data -> 
          ZProperty.make id data))
  
  

let encode_property (id, value) buf =  
  Apero.fast_encode_vle id buf;
  Apero.encode_buf value buf