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
open Sexplib.Std
open Printf


module Node = struct
  type id = string [@@deriving sexp, yojson]

  type t = {
    node_id  : id;
    tree_nb  : int;
    priority : (int * id);
    distance : int;
    parent   : id option;
    rank     : int
  }  [@@deriving sexp, yojson]

  let compare t1 t2 =
    let c1 = compare t1.rank t2.rank in
    if c1 <> 0 then c1 else 
    begin
      let c2 = compare t1.priority t2.priority in
      if c2 <> 0 then c2 else compare t2.distance t1.distance
    end
end

type t = {
  local : Node.t;
  peers : Node.t list;
} [@@deriving yojson]
type tree=t [@@deriving yojson]

module type S = sig
  val update : t -> Node.t -> t
  val delete_node : t -> Node.id -> t
  val is_stable : t -> bool
  val get_parent : t -> Node.t option
  val get_childs : t -> Node.t list
  val get_broken_links : t -> Node.t list
  val report : t -> string
end

module type Configuration = sig
  val local_id : Node.id
  val local_prio : int
  val max_dist : int
  val max_trees : int
end

module Configure(Conf : Configuration) : S = struct
  open Node

  let rec best_peer peers = match peers with
    | [] -> invalid_arg "empty list"
    | x :: [] -> x
    | x :: remain ->
      let max_rem = best_peer remain in
      if compare x max_rem > 0 then x else max_rem

  let update tree node =
    let open Node in
    let peers =
      match List.find_opt (fun peer -> peer.node_id = node.node_id) tree.peers with
      | None -> node :: tree.peers
      | Some _ -> List.map (fun peer -> if peer.node_id = node.node_id then node else peer) tree.peers in
    let self = {
        node_id  = Conf.local_id;
        tree_nb  = tree.local.tree_nb;
        priority = (Conf.local_prio, Conf.local_id);
        distance = 0;
        parent   = None;
        rank     = tree.local.rank;
      } in
    let best = best_peer (self::peers) in
    let local = if best.node_id = Conf.local_id
    then self
    else {best with node_id = Conf.local_id; parent = Some best.node_id;} in
    {local; peers}

  let delete_node tree nodeid =
    match List.find_opt (fun peer -> (peer.node_id = nodeid)) tree.peers with
    | None -> tree
    | Some node ->
      let peers = List.filter (fun peer -> (peer.node_id <> nodeid)) tree.peers in
      match node.parent with 
      | Some _ ->
        let self = {
            node_id  = Conf.local_id;
            tree_nb  = tree.local.tree_nb;
            priority = (Conf.local_prio, Conf.local_id);
            distance = 0;
            parent   = None;
            rank     = tree.local.rank;
          } in
        let best = best_peer (self::peers) in
        let local = if best.node_id = Conf.local_id
        then self
        else {best with node_id = Conf.local_id; parent = Some best.node_id;} in
        {local; peers}
      | None ->
        let self = {
            node_id  = Conf.local_id;
            tree_nb  = tree.local.tree_nb;
            priority = (Conf.local_prio, Conf.local_id);
            distance = 0;
            parent   = None;
            rank     = tree.local.rank + 1;
          } in
        {local=self; peers}

  let is_stable tree =
    List.for_all (fun peer -> peer.priority = tree.local.priority) tree.peers

  let get_parent tree =
    match tree.local.parent with
    | None -> None
    | Some parent ->
    List.find_opt (fun peer -> peer.node_id = parent) tree.peers

  let get_childs tree =
    List.filter (fun peer -> 
      match peer.parent with
      | None -> false
      | Some parent -> parent = tree.local.node_id) tree.peers

  let get_broken_links tree =
    tree.peers
    |> List.filter (fun (peer:Node.t) -> 
        match peer.parent with
        | None -> false
        | Some parent -> parent <> tree.local.node_id)
    |> List.filter (fun (peer:Node.t) -> 
        match get_parent tree with
        | None -> true
        | Some parent -> parent.node_id <> peer.node_id)

  let report tree =
    sprintf "   Local : %s\n" (Sexplib.Sexp.to_string (Node.sexp_of_t tree.local)) ^
    sprintf "      Parent      : %s\n%!"
      (match get_parent tree with
      | None -> "none"
      | Some parent -> (Sexplib.Sexp.to_string (Node.sexp_of_t (parent)))) ^
    List.fold_left (fun s peer -> sprintf "%s      Children    : %s\n" s (Sexplib.Sexp.to_string (Node.sexp_of_t peer))) "" (get_childs tree) ^
    List.fold_left (fun s peer -> sprintf "%s      Broken link : %s\n" s (Sexplib.Sexp.to_string (Node.sexp_of_t peer))) ""  (get_broken_links tree)
end

module Set = struct
  
  type t = tree list [@@deriving yojson]

  module type S = sig
    include S
    val create : t
    val is_stable : t -> bool
    val get_tree : t -> int -> tree option
    val min_dist : t -> int
    val next_tree : 'a list -> int
    val update_tree_set : t -> Node.t -> t
    val delete_node : t -> Node.id -> t
    val report : t -> string
  end

  module Configure(Conf : Configuration) : S = struct
    module Tree = Configure(Conf)

    include Tree

    let create =
      [{
        local = 
        {
          node_id  = Conf.local_id;
          tree_nb  = 0;
          priority = (Conf.local_prio, Conf.local_id);
          distance = 0;
          parent   = None;
          rank     = 0 
        };
        peers = []
      }]

    let is_stable tree_set =
      List.for_all (fun x -> Tree.is_stable x) tree_set

    let get_tree tree_set tid = 
      List.find_opt (fun tree -> tree.local.tree_nb = tid) tree_set

    let min_dist tree_set =
      (List.fold_left (fun a b -> if a.local.distance < b.local.distance then a else b) (List.hd tree_set) tree_set).local.distance

    let next_tree tree_set =
      List.length tree_set

    let update_tree_set tree_set node =
      let open Node in
      let tree_set = match List.exists (fun tree -> tree.local.tree_nb = node.tree_nb) tree_set with
      | true -> tree_set
      | false ->
        {
          local =
            {
              node_id  = Conf.local_id;
              tree_nb  = node.tree_nb;
              distance = 0;
              parent   = None;
              rank     = node.rank;
              priority = match (min_dist tree_set > Conf.max_dist) with
              | true -> (Conf.local_prio, Conf.local_id)
              | false -> (0, Conf.local_id) (*TODO *)
            };
          peers = [];
        } :: tree_set in
      let tree_set =
        List.map (fun tree -> 
          if tree.local.tree_nb = node.tree_nb then Tree.update tree node else tree)
          tree_set in
      let tree_set = match is_stable tree_set && List.length tree_set < Conf.max_trees with
      | false -> tree_set
      | true -> match min_dist tree_set > Conf.max_dist with
        | false -> tree_set
        | true ->
          {
            local =
              {
                node_id  = Conf.local_id;
                tree_nb  = next_tree tree_set;
                distance = 0;
                parent   = None;
                rank     = 0;
                priority = (Conf.local_prio, Conf.local_id)
              };
            peers = [];
          } :: tree_set in
      tree_set

    let delete_node tree_set node =
      List.map (fun x -> Tree.delete_node x node) tree_set

    let report tree_set =
      tree_set
      |> List.sort (fun a b -> compare a.local.tree_nb b.local.tree_nb)
      |> List.fold_left (fun s tree -> sprintf "%sTree nb %i:\n%s" s tree.local.tree_nb (report tree)) ""
  end
end
