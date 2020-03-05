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

module Qid = struct
    type t = (Abuf.t * Vle.t)
    let compare (pid1, qid1) (pid2, qid2) = 
        let c1 = compare (Vle.to_int qid1) (Vle.to_int qid2) in
        if c1 <> 0 then c1 else compare (Abuf.hexdump pid1) (Abuf.hexdump pid2)
end

type t = {
    srcFace : NetService.Id.t;
    fwdFaces : NetService.Id.t list;
}