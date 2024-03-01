use ahash::RandomState;
use std::collections::HashMap;
use zenoh_config::{
    AclConfig, Action, ConfigRule, Permission, PolicyRule, Subject, NUMBER_OF_ACTIONS,
    NUMBER_OF_PERMISSIONS,
};
use zenoh_keyexpr::keyexpr;
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, KeBoxTree};
use zenoh_result::ZResult;

pub struct PolicyForSubject(Vec<Vec<KeTreeRule>>); //vec of actions over vec of permission for tree of ke for this
pub struct PolicyList(pub HashMap<i32, PolicyForSubject, RandomState>); //index of subject_map instead of subject

type KeTreeRule = KeBoxTree<bool>;

pub struct PolicyEnforcer {
    pub(crate) acl_enabled: bool,
    pub(crate) default_permission: Permission,
    pub(crate) subject_map: Option<HashMap<Subject, i32, RandomState>>,
    pub(crate) policy_list: Option<PolicyList>,
}

#[derive(Debug, Clone)]

pub struct PolicyInformation {
    subject_map: HashMap<Subject, i32, RandomState>,
    policy_rules: Vec<PolicyRule>,
}

impl PolicyEnforcer {
    pub fn new() -> PolicyEnforcer {
        PolicyEnforcer {
            acl_enabled: true,
            default_permission: Permission::Deny,
            subject_map: None,
            policy_list: None,
        }
    }

    pub fn init(&mut self, acl_config: AclConfig) -> ZResult<()> {
        self.acl_enabled = acl_config.enabled;
        self.default_permission = acl_config.default_permission;
        if self.acl_enabled {
            if let Some(rules) = acl_config.rules {
                if rules.is_empty() {
                    log::warn!("ACL ruleset in config file is empty!!!");
                    self.policy_list = None;
                    self.subject_map = None;
                }
                let policy_information = self.policy_information_point(rules)?;

                let subject_map = policy_information.subject_map;
                let mut main_policy: PolicyList = PolicyList(HashMap::default());
                //first initialize the vector of vectors (required to maintain the indices)
                for index in subject_map.values() {
                    let mut rule: PolicyForSubject = PolicyForSubject(Vec::new());
                    for _i in 0..NUMBER_OF_ACTIONS {
                        let mut action_rule: Vec<KeTreeRule> = Vec::new();
                        for _j in 0..NUMBER_OF_PERMISSIONS {
                            let permission_rule = KeTreeRule::new();
                            //
                            action_rule.push(permission_rule);
                        }
                        rule.0.push(action_rule);
                    }
                    main_policy.0.insert(*index, rule);
                }

                for rule in policy_information.policy_rules {
                    //add key-expression values to the ketree as per the policy rules
                    if let Some(index) = subject_map.get(&rule.subject) {
                        if let Some(single_policy) = main_policy.0.get_mut(index) {
                            single_policy.0[rule.action as usize][rule.permission as usize]
                                .insert(keyexpr::new(&rule.key_expr)?, true);
                        }
                    };
                }
                //add to the policy_enforcer
                self.policy_list = Some(main_policy);
                self.subject_map = Some(subject_map);
            } else {
                log::warn!("No ACL rules have been specified!!!");
            }
        }
        Ok(())
    }
    pub fn policy_information_point(
        &self,
        config_rule_set: Vec<ConfigRule>,
    ) -> ZResult<PolicyInformation> {
        /*
           get the list of policies from the config PolicyList
           convert them into the subject format for the vec of rules
           send the vec as part of policy information
           also take the subject values to create the subject_map and pass that as part of poliy infomration
        */
        //we need to convert the sets of rules into individual rules for each subject, key-expr, action, permission
        let mut policy_rules: Vec<PolicyRule> = Vec::new();
        for config_rule in config_rule_set {
            for subject in &config_rule.interface {
                for action in &config_rule.action {
                    for key_expr in &config_rule.key_expr {
                        policy_rules.push(PolicyRule {
                            subject: Subject::Interface(subject.clone()),
                            key_expr: key_expr.clone(),
                            action: action.clone(),
                            permission: config_rule.permission.clone(),
                        })
                    }
                }
            }
        }
        let mut subject_map = HashMap::default();
        let mut counter = 1; //starting at 1 since 0 is initialized value in policy_check and should not match anything
        for rule in policy_rules.iter() {
            subject_map.insert(rule.subject.clone(), counter);
            counter += 1;
        }
        Ok(PolicyInformation {
            subject_map,
            policy_rules,
        })
    }

    /*
       checks each msg against the ACL ruleset for allow/deny
    */

    pub fn policy_decision_point(
        &self,
        subject: i32,
        action: Action,
        key_expr: &str,
        default_decision: bool,
    ) -> ZResult<bool> {
        match &self.policy_list {
            Some(policy_map) => {
                //let single_policy = policy_map.0.get(&subject).unwrap();
                match policy_map.0.get(&subject) {
                    Some(single_policy) => {
                        let perm_vec = &single_policy.0[action as usize];

                        //explicit Deny rules are ALWAYS given preference
                        let deny_result = perm_vec[Permission::Deny as usize]
                            .nodes_including(keyexpr::new(&key_expr)?)
                            .count();
                        if deny_result != 0 {
                            return Ok(false);
                        }
                        //if default_permission is Allow, ignore checks for Allow
                        if self.default_permission == Permission::Allow {
                            Ok(true)
                        } else {
                            let allow_result = perm_vec[Permission::Allow as usize]
                                .nodes_including(keyexpr::new(&key_expr)?)
                                .count();
                            Ok(allow_result != 0)
                        }
                    }
                    None => Ok(default_decision),
                }
            }
            None => {
                //when list is empty
                if self.default_permission == Permission::Allow {
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    }
}
