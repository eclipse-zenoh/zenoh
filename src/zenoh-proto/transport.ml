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
open Apero_net
open Frame


module Transport = struct

  module Id = NetService.Id 

  module  Info = struct
    type kind = Packet| Stream  
    type t = {name : string; id : Id.t; reliable : bool; kind : kind; mtu : int option }
    let create name id reliable kind mtu = {name; id; reliable; kind; mtu}
    let name i = i.name 
    let id i = i.id 
    let reliable i = i.reliable
    let kind i = i.kind
    let mtu i = i.mtu 
    let compare a b = compare a b
  end

  module Session = struct 
    module Id = NetService.Id
    module Info = struct 
      type t = { id : Id.t; src : Locator.t; dest : Locator.t; transport_info : Info.t }

      let create id src dest transport_info = {id; src; dest; transport_info}
      let id si = si.id
      let source si = si.src
      let dest si = si.dest 
      let transport_info si = si.transport_info
    end
  end


   
  module Event = struct
    type event = 
      | SessionClose of Session.Id.t
      | SessionMessage of  Frame.t * Session.Id.t * (push option)
      | LocatorMessage of Frame.t * Locator.t * (push option)
      | Events of event list 
    and  pull = unit -> event Lwt.t
    and  push = event -> unit Lwt.t
  end
  
  module type S = sig 
    val start : Event.push -> unit Lwt.t
    val stop : unit -> unit Lwt.t
    val info : Info.t  
    val listen : Locator.t -> Session.Id.t Lwt.t
    val connect : Locator.t -> (Session.Id.t * Event.push) Lwt.t
    val session_info : Session.Id.t -> Session.Info.t option
  end  

  module Engine = struct
    open Lwt
    type t = int   

    let create () = Lwt.return 0
    let add_transport _ _ = return Id.zero 
    let remove_transport _ _ = return false
    let listen _ _ = fail @@ Exception `NotImplemented
    let connect _ _ = fail @@ Exception `NotImplemented
    let start _ _ = fail @@ Exception `NotImplemented
    let session_info _ _ = None    
  end
end
