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
module Frame = struct
  type t = {msgs : Message.t list}
  let empty = {msgs = [];}
  let create msgs = {msgs}
  let add msg f = {msgs = msg::(f.msgs)}
  let length f = List.length f.msgs
  let to_list f = f.msgs
  let from_list msgs = {msgs}
end
