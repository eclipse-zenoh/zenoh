use std::fmt;

// use casbin::{CoreApi, Enforcer};
use super::RoutingContext;
use casbin::prelude::*;
use zenoh_keyexpr::keyexpr_tree::box_tree::KeBoxTree;
use zenoh_protocol::network::NetworkMessage;
use zenoh_result::ZResult;

pub enum Action {
    NONE,
    READ,
    WRITE,
    READWRITE,
    SUBSCRIBEDECLARE,
}

impl fmt::Debug for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub trait ZAuth {
    fn authz_testing(&self, _: String, _: String, _: String) -> ZResult<bool>;
}

impl ZAuth for Enforcer {
    fn authz_testing(&self, zid: String, ke: String, act: String) -> ZResult<bool> {
        /*
        (zid, keyexpr, act): these values should be extraced from the authn code.
        has to be atomic, to avoid another process sending the wrong info
         */

        if let Ok(authorized) = self.enforce((zid.clone(), ke.clone(), act.clone())) {
            Ok(authorized)
        } else {
            println!("policy enforcement error");
            Ok(false)
        }
    }
}

pub async fn start_authz() -> Result<Enforcer> {
    // get file value
    let mut e = Enforcer::new("keymatch_model.conf", "keymatch_policy.csv").await?;
    e.enable_log(true);
    Ok(e)
}

struct StaticPolicy {
    //should have values needed for the policy to work
    //config for local static policy
}

//policy builder (add policy type and ruleset)

//subject_builder (add ID, attributes, roles)

//request_builder (add subject, resource, action) //do we need one?

pub struct Subject {
    //ID and list of possible policy attributes
    id: Option<String>, //can convert uuid to string and back
    attributes: Option<Vec<String>>,
}

pub struct PolicyEnforcer {
    policy_config: String,
}

impl PolicyEnforcer {
    pub fn init_policy(pol_string: String) -> ZResult<Self> {
        let pe = Self {
            policy_config: pol_string,
        };
        Ok(pe)
    }
    pub fn policy_enforcement_point(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        action: Action,
    ) -> ZResult<bool> {
        /*
           input: msg body
           output: allow deny
           function: depending on the msg, calls build_permission_request(msg,action), passes its output to policy_decision_point()
                    uses allow/deny output to block or pass the msg to routing table
        */
        let ke = ctx.full_expr().unwrap().to_owned();
        let zid = ctx.inface().unwrap().state.zid;
        //build subject here
        //for now just set it
        let s = Subject {
            id: Some(zid.to_string()),
            attributes: None,
        };

        let decision = self.policy_decision_point(s, ke, action);
        Ok(false)
    }
    pub fn permission_request_builder(
        msg: zenoh_protocol::network::NetworkMessage,
        action: Action,
    ) {

        /*
           input: msg body
           output: (sub,ke,act)
           function: extract relevant info from the incoming msg body
                        build the subject [ID, Attributes and Roles]
                        then use that to build the request [subject, key-expression, action ]
                        return request to PEP
        */
        /*
           PHASE1: just extract the ID (zid?) from the msg; can later add attributes to the list. have a struct with ID and attributes field (both Option)
        */
    }
    pub fn policy_decision_point(&self, subject: Subject, ke: String, action: u8) -> ZResult<bool> {
        /*
            input: (sub,ke,act)
            output: true(allow)/false(deny)
            function: process the request from PEP against policy list
                    policy list will(might) be a hashmap of subject:rules_vector (test and discuss)
        */
        /*
           PHASE1: policy decisions are hardcoded against the policy list; can later update them using a config file.
        */
        //representaiton of policy list as a hashmap of trees?
        // HashMap<id,Hashmap<action,KeBoxTree>>
        //use KeTrees for mapping R/W values? //need to check this out
        Ok(false)
    }

    pub fn policy_resource_point() {

        /*
           input: config file value along with &self
           output: loads the appropriate policy into the memory and returns back self (now with added policy info); might also select AC type (ACL or ABAC)
        */

        /*
           PHASE1: just have a vector of structs containing these values; later we can load them here from config
        */
    }
}

pub fn testing_ke() {}
