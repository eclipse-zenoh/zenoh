//
// Copyright (c) 2024 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
use ahash::RandomState;
use enum_map::{enum_map, EnumMap};
use std::collections::HashMap;
use zenoh_config::{AclConfig, AclConfigRules, Action, Flow, Permission, PolicyRule, Subject};
use zenoh_keyexpr::keyexpr;
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, KeBoxTree};
use zenoh_result::ZResult;
type PermissionVec = Vec<KeTreeRule>;
type ActionVec = Vec<PermissionVec>;
type PolicyForSubject = Vec<ActionVec>;
type PolicyMap = HashMap<i32, PolicyForSubject, RandomState>;
type SubjectMap = HashMap<Subject, i32, RandomState>;
type KeTreeRule = KeBoxTree<bool>;
pub struct PolicyEnforcer {
    pub(crate) acl_enabled: bool,
    pub(crate) default_permission: Permission,
    pub(crate) subject_map: SubjectMap,
    pub(crate) policy_map: PolicyMap,
}

#[derive(Debug, Clone)]
pub struct PolicyInformation {
    subject_map: SubjectMap,
    policy_rules: Vec<PolicyRule>,
}

impl PolicyEnforcer {
    pub fn new() -> PolicyEnforcer {
        PolicyEnforcer {
            acl_enabled: true,
            default_permission: Permission::Deny,
            subject_map: SubjectMap::default(),
            policy_map: PolicyMap::default(),
        }
    }

    /*
       initializes the policy_enforcer
    */
    pub fn init(&mut self, acl_config: AclConfig) -> ZResult<()> {
        let (action_index, permission_index, flow_index) = get_enum_map();
        let (number_of_actions, number_of_permissions, number_of_flows) =
            (action_index.len(), permission_index.len(), flow_index.len());

        self.acl_enabled = acl_config.enabled;
        self.default_permission = acl_config.default_permission;
        if self.acl_enabled {
            if let Some(rules) = acl_config.rules {
                if rules.is_empty() {
                    log::warn!("[ACCESS LOG]: ACL ruleset in config file is empty!!!");
                    self.policy_map = PolicyMap::default();
                    self.subject_map = SubjectMap::default();
                } else {
                    let policy_information = self.policy_information_point(rules)?;
                    let subject_map = policy_information.subject_map;
                    let mut main_policy: PolicyMap = PolicyMap::default();

                    for index in subject_map.values() {
                        let mut rule = PolicyForSubject::new();
                        for _k in 0..number_of_flows {
                            let mut flow_rule = Vec::new();
                            for _i in 0..number_of_actions {
                                let mut action_rule: Vec<KeTreeRule> = Vec::new();
                                for _j in 0..number_of_permissions {
                                    let permission_rule = KeTreeRule::new();
                                    action_rule.push(permission_rule);
                                }
                                flow_rule.push(action_rule);
                            }
                            rule.push(flow_rule);
                        }
                        main_policy.insert(*index, rule);
                    }

                    for rule in policy_information.policy_rules {
                        if let Some(index) = subject_map.get(&rule.subject) {
                            if let Some(single_policy) = main_policy.get_mut(index) {
                                single_policy[flow_index[rule.flow]][action_index[rule.action]]
                                    [permission_index[rule.permission]]
                                    .insert(keyexpr::new(&rule.key_expr)?, true);
                            }
                        };
                    }
                    self.policy_map = main_policy;
                    self.subject_map = subject_map;
                }
            } else {
                log::warn!("[ACCESS LOG]: No ACL rules have been specified!!!");
            }
        }
        Ok(())
    }

    /*
       converts the sets of rules from config format into individual rules for each subject, key-expr, action, permission
    */
    pub fn policy_information_point(
        &self,
        config_rule_set: Vec<AclConfigRules>,
    ) -> ZResult<PolicyInformation> {
        let mut policy_rules: Vec<PolicyRule> = Vec::new();
        for config_rule in config_rule_set {
            for subject in &config_rule.interface {
                for flow in &config_rule.flow {
                    for action in &config_rule.action {
                        for key_expr in &config_rule.key_expr {
                            policy_rules.push(PolicyRule {
                                subject: Subject::Interface(subject.clone()),
                                key_expr: key_expr.clone(),
                                action: action.clone(),
                                permission: config_rule.permission.clone(),
                                flow: flow.clone(),
                            })
                        }
                    }
                }
            }
        }
        let mut subject_map = SubjectMap::default();
        let mut counter = 1; //starting at 1 since 0 is the init value and should not match anything
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
        flow: Flow,
        action: Action,
        key_expr: &str,
    ) -> ZResult<Permission> {
        let policy_map = &self.policy_map;

        let (action_index, permission_index, flow_index) = get_enum_map();
        match policy_map.get(&subject) {
            Some(single_policy) => {
                let deny_result = single_policy[flow.clone() as usize][action.clone() as usize]
                    [Permission::Deny as usize]
                    .nodes_including(keyexpr::new(&key_expr)?)
                    .count();
                if deny_result != 0 {
                    return Ok(Permission::Deny);
                }
                if self.default_permission == Permission::Allow {
                    Ok(Permission::Allow)
                } else {
                    let allow_result = single_policy[flow_index[flow]][action_index[action]]
                        [permission_index[Permission::Allow]]
                        .nodes_including(keyexpr::new(&key_expr)?)
                        .count();

                    if allow_result != 0 {
                        Ok(Permission::Allow)
                    } else {
                        Ok(Permission::Deny)
                    }
                }
            }
            None => Ok(self.default_permission.clone()),
        }
    }
}

fn get_enum_map() -> (
    EnumMap<Action, usize>,
    EnumMap<Permission, usize>,
    EnumMap<Flow, usize>,
) {
    let action_index = enum_map! {
        Action::Put => 0_usize,
        Action::DeclareSubscriber => 1,
        Action::Get => 2,
        Action::DeclareQueryable => 3,
    };
    let permission_index = enum_map! {
        Permission::Allow =>  0_usize,
        Permission::Deny => 1
    };
    let flow_index = enum_map! {
        Flow::Egress =>  0_usize,
        Flow::Ingress => 1
    };
    (action_index, permission_index, flow_index)
}
