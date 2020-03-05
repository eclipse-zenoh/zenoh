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

module ZConduit = struct
  type t = {
    id : int;
    mutable rsn : Vle.t;
    mutable usn : Vle.t;
    
  }
  let make id = { id ; rsn = 0L; usn = 0L}
  let next_rsn c =
    let n = c.rsn in c.rsn <- Vle.add c.rsn 1L ; n

  let next_usn c =
    let n = c.usn in c.usn <- Vle.add c.usn 1L ; n

  let id c = c.id
end
