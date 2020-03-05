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
open Frame
open Locator

module Transport : sig 

  module Id : Apero.NumId.S

  module  Info : sig 
      type kind = Packet| Stream  
      type t      
      val create : string -> Id.t -> bool -> kind -> int option -> t
      val name : t -> string
      val id : t -> Id.t
      val reliable : t -> bool
      val kind : t -> kind
      val mtu : t -> int option
      val compare : t -> t -> int
  end
  
  module Session : sig 
    
    module Id : Apero.NumId.S

    module Info : sig  
      type t
      val create : Id.t -> Locator.t -> Locator.t -> Info.t -> t
      val id : t -> Id.t
      val source : t -> Locator.t
      val dest : t -> Locator.t
      val transport_info : t -> Info.t
    end
  end

  module Event : sig
    type event = 
      | SessionClose of Session.Id.t
      | SessionMessage of  Frame.t * Session.Id.t * (push option)
      | LocatorMessage of Frame.t * Locator.t * (push  option)
      | Events of event list 
    and  pull = unit -> event Lwt.t
    and  push = event -> unit Lwt.t
  end
  
  module type S = sig
    val start : Event.push ->  unit Lwt.t
    val stop : unit -> unit Lwt.t
    val info : Info.t      
    val listen : Locator.t -> Session.Id.t Lwt.t
    val connect : Locator.t -> (Session.Id.t * Event.push) Lwt.t
    val session_info : Session.Id.t -> Session.Info.t option
  end  

  module Engine : sig
  (** The [Transport.Engine] provides facilities for dyanmically loading transports 
      and abstracting them. *)
    type t        

    val create : unit -> t Lwt.t
    val add_transport : t -> (module S) -> Id.t Lwt.t
    val remove_transport : t -> Id.t -> bool Lwt.t
    val listen : t -> Locator.t -> Session.Id.t Lwt.t
    val connect : t -> Locator.t -> Session.Id.t Lwt.t
    val start : t -> Event.pull -> Event.push Lwt.t     
    val session_info : t -> Session.Id.t -> Session.Info.t option
  end
end
