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
type local_sex = (Frame.Frame.t Lwt_stream.t * Frame.Frame.t Lwt_stream.bounded_push)
type local_sex_listener = local_sex -> unit Lwt.t

val register_router : local_sex_listener -> unit Lwt.t

val open_local_session : unit -> local_sex option Lwt.t