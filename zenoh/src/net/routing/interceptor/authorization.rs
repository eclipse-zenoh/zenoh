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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use std::collections::{HashMap, HashSet};

use ahash::RandomState;
use itertools::Itertools;
use zenoh_config::{
    AclConfig, AclConfigPolicyEntry, AclConfigRule, AclConfigSubjects, AclMessage, CertCommonName,
    InterceptorFlow, InterceptorLink, Interface, Permission, PolicyRule, Username, ZenohId,
};
use zenoh_keyexpr::{
    keyexpr,
    keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, IKeyExprTreeNode, KeBoxTree},
    OwnedKeyExpr,
};
use zenoh_result::ZResult;

use super::InterfaceEnabled;
type PolicyForSubject = FlowPolicy;

type PolicyMap = HashMap<usize, PolicyForSubject, RandomState>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Subject {
    pub(crate) interface: SubjectProperty<Interface>,
    pub(crate) cert_common_name: SubjectProperty<CertCommonName>,
    pub(crate) username: SubjectProperty<Username>,
    pub(crate) link_type: SubjectProperty<InterceptorLink>,
    pub(crate) zid: SubjectProperty<ZenohId>,
}

impl Subject {
    fn matches(&self, query: &SubjectQuery) -> bool {
        self.interface.matches(query.interface.as_ref())
            && self.username.matches(query.username.as_ref())
            && self
                .cert_common_name
                .matches(query.cert_common_name.as_ref())
            && self.link_type.matches(query.link_protocol.as_ref())
            && self.zid.matches(query.zid.as_ref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum SubjectProperty<T> {
    Wildcard,
    Exactly(T),
    Prefix(T),
}

/// Prefix matching support for subject properties. Only meaningful for string-like
/// properties (currently certificate common names); other properties never prefix-match.
pub(crate) trait SubjectPrefixMatch {
    fn has_prefix(&self, _prefix: &Self) -> bool {
        false
    }
}

impl SubjectPrefixMatch for CertCommonName {
    fn has_prefix(&self, prefix: &Self) -> bool {
        self.0.starts_with(&prefix.0)
    }
}

impl SubjectPrefixMatch for Interface {}
impl SubjectPrefixMatch for Username {}
impl SubjectPrefixMatch for InterceptorLink {}
impl SubjectPrefixMatch for ZenohId {}

impl<T: PartialEq + Eq + SubjectPrefixMatch> SubjectProperty<T> {
    fn matches(&self, other: Option<&T>) -> bool {
        match (self, other) {
            (SubjectProperty::Wildcard, None) => true,
            // NOTE: This match arm is the reason why `SubjectProperty` cannot simply be `Option`
            (SubjectProperty::Wildcard, Some(_)) => true,
            (SubjectProperty::Exactly(_), None) => false,
            (SubjectProperty::Exactly(lhs), Some(rhs)) => lhs == rhs,
            (SubjectProperty::Prefix(_), None) => false,
            (SubjectProperty::Prefix(lhs), Some(rhs)) => rhs.has_prefix(lhs),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SubjectQuery {
    pub(crate) interface: Option<Interface>,
    pub(crate) cert_common_name: Option<CertCommonName>,
    pub(crate) username: Option<Username>,
    pub(crate) link_protocol: Option<InterceptorLink>,
    pub(crate) zid: Option<ZenohId>,
}

impl std::fmt::Display for SubjectQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let subject_names = [
            self.interface.as_ref().map(|face| format!("{face}")),
            self.cert_common_name.as_ref().map(|ccn| format!("{ccn}")),
            self.username.as_ref().map(|username| format!("{username}")),
            self.link_protocol.as_ref().map(|link| format!("{link}")),
            self.zid.as_ref().map(|zid| format!("{zid}")),
        ];
        write!(
            f,
            "{}",
            subject_names
                .iter()
                .filter_map(|v| v.as_ref())
                .cloned()
                .collect::<Vec<_>>()
                .join("+")
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SubjectEntry {
    pub(crate) subject: Subject,
    pub(crate) id: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct SubjectStore {
    inner: Vec<SubjectEntry>,
}

impl SubjectStore {
    pub(crate) fn query<'a: 'b, 'b>(
        &'a self,
        query: &'b SubjectQuery,
    ) -> impl Iterator<Item = &'a SubjectEntry> + 'b {
        // FIXME: Can this search be better than linear?
        self.inner
            .iter()
            .filter(|entry| entry.subject.matches(query))
    }
}

impl Default for SubjectStore {
    fn default() -> Self {
        SubjectMapBuilder::new().build()
    }
}

pub(crate) struct SubjectMapBuilder {
    builder: HashMap<Subject, usize>,
    id_counter: usize,
}

impl SubjectMapBuilder {
    pub(crate) fn new() -> Self {
        Self {
            // FIXME: Capacity can be calculated from the length of subject properties in configuration
            builder: HashMap::new(),
            id_counter: 0,
        }
    }

    pub(crate) fn build(self) -> SubjectStore {
        SubjectStore {
            inner: self
                .builder
                .into_iter()
                .map(|(subject, id)| SubjectEntry { subject, id })
                .collect(),
        }
    }

    /// Assumes subject contains at most one instance of each Subject variant
    pub(crate) fn insert_or_get(&mut self, subject: Subject) -> usize {
        match self.builder.get(&subject).copied() {
            Some(id) => id,
            None => {
                self.id_counter += 1;
                self.builder.insert(subject, self.id_counter);
                self.id_counter
            }
        }
    }
}

type KeTreeRule = KeBoxTree<bool>;

#[derive(Default)]
struct PermissionPolicy {
    allow: KeTreeRule,
    deny: KeTreeRule,
}

impl PermissionPolicy {
    fn permission(&self, permission: Permission) -> &KeTreeRule {
        match permission {
            Permission::Allow => &self.allow,
            Permission::Deny => &self.deny,
        }
    }
    fn permission_mut(&mut self, permission: Permission) -> &mut KeTreeRule {
        match permission {
            Permission::Allow => &mut self.allow,
            Permission::Deny => &mut self.deny,
        }
    }
}
#[derive(Default)]
struct ActionPolicy {
    query: PermissionPolicy,
    put: PermissionPolicy,
    delete: PermissionPolicy,
    declare_subscriber: PermissionPolicy,
    declare_queryable: PermissionPolicy,
    reply: PermissionPolicy,
    liveliness_token: PermissionPolicy,
    declare_liveliness_sub: PermissionPolicy,
    liveliness_query: PermissionPolicy,
}

impl ActionPolicy {
    fn action(&self, action: AclMessage) -> &PermissionPolicy {
        match action {
            AclMessage::Query => &self.query,
            AclMessage::Reply => &self.reply,
            AclMessage::Put => &self.put,
            AclMessage::Delete => &self.delete,
            AclMessage::DeclareSubscriber => &self.declare_subscriber,
            AclMessage::DeclareQueryable => &self.declare_queryable,
            AclMessage::LivelinessToken => &self.liveliness_token,
            AclMessage::DeclareLivelinessSubscriber => &self.declare_liveliness_sub,
            AclMessage::LivelinessQuery => &self.liveliness_query,
        }
    }
    fn action_mut(&mut self, action: AclMessage) -> &mut PermissionPolicy {
        match action {
            AclMessage::Query => &mut self.query,
            AclMessage::Reply => &mut self.reply,
            AclMessage::Put => &mut self.put,
            AclMessage::Delete => &mut self.delete,
            AclMessage::DeclareSubscriber => &mut self.declare_subscriber,
            AclMessage::DeclareQueryable => &mut self.declare_queryable,
            AclMessage::LivelinessToken => &mut self.liveliness_token,
            AclMessage::DeclareLivelinessSubscriber => &mut self.declare_liveliness_sub,
            AclMessage::LivelinessQuery => &mut self.liveliness_query,
        }
    }
}

#[derive(Default)]
pub struct FlowPolicy {
    ingress: ActionPolicy,
    egress: ActionPolicy,
}

impl FlowPolicy {
    fn flow(&self, flow: InterceptorFlow) -> &ActionPolicy {
        match flow {
            InterceptorFlow::Ingress => &self.ingress,
            InterceptorFlow::Egress => &self.egress,
        }
    }
    fn flow_mut(&mut self, flow: InterceptorFlow) -> &mut ActionPolicy {
        match flow {
            InterceptorFlow::Ingress => &mut self.ingress,
            InterceptorFlow::Egress => &mut self.egress,
        }
    }
}

/// Placeholder replaced by the authenticated certificate common name of the remote
/// instance when a `key_expr_templates` rule is expanded at transport establishment.
pub(crate) const KE_TEMPLATE_CERT_COMMON_NAME: &str = "${cert_common_name}";

/// Placeholder replaced by the authenticated username (user/password authentication)
/// of the remote instance when a `key_expr_templates` rule is expanded.
pub(crate) const KE_TEMPLATE_USERNAME: &str = "${username}";

/// The authenticated identity attributes of a connection available for template
/// expansion. An attribute is `None` when the corresponding authentication method
/// was not used on the connection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub(crate) struct TemplateVars {
    pub(crate) cert_common_name: Option<String>,
    pub(crate) username: Option<String>,
}

/// A rule carrying key expression templates instead of static key expressions.
/// It is not expanded into `PolicyMap` at config time: the actual key expressions
/// are only known per-connection, once the remote identity is authenticated.
#[derive(Debug, Clone)]
pub(crate) struct TemplatePolicyRule {
    pub(crate) subject_id: usize,
    pub(crate) template: String,
    pub(crate) message: AclMessage,
    pub(crate) permission: Permission,
    pub(crate) flow: InterceptorFlow,
}

/// Checks that a value is safe to substitute into a key expression template as
/// (part of) a single chunk: it must not inject wildcards or chunk separators.
fn is_valid_template_substitution(value: &str) -> bool {
    !value.is_empty() && !value.contains(['/', '*', '$', '?', '#'])
}

/// Expands a key expression template with the authenticated identity attributes of a
/// connection and validates the result as a key expression. Fails if the template
/// references an attribute the connection does not have, or if a value is not safe
/// to substitute.
pub(crate) fn expand_template(template: &str, vars: &TemplateVars) -> ZResult<OwnedKeyExpr> {
    let mut expanded = template.to_string();
    for (placeholder, value) in [
        (KE_TEMPLATE_CERT_COMMON_NAME, &vars.cert_common_name),
        (KE_TEMPLATE_USERNAME, &vars.username),
    ] {
        if !expanded.contains(placeholder) {
            continue;
        }
        let Some(value) = value else {
            bail!(
                "template references {} but the connection has no such authenticated identity",
                placeholder
            );
        };
        if !is_valid_template_substitution(value) {
            bail!(
                "value {:?} is not safe to substitute into a key expression template",
                value
            );
        }
        expanded = expanded.replace(placeholder, value);
    }
    OwnedKeyExpr::autocanonize(expanded.clone()).map_err(|e| {
        zerror!(
            "template expansion produced invalid key expression {:?}: {}",
            expanded,
            e
        )
        .into()
    })
}

pub struct PolicyEnforcer {
    pub(crate) acl_enabled: bool,
    pub(crate) default_permission: Permission,
    pub(crate) subject_store: SubjectStore,
    pub(crate) policy_map: PolicyMap,
    pub(crate) template_rules: Vec<TemplatePolicyRule>,
    pub(crate) interface_enabled: InterfaceEnabled,
}

#[derive(Debug, Clone)]
pub struct PolicyInformation {
    subject_map: SubjectStore,
    policy_rules: Vec<PolicyRule>,
    template_rules: Vec<TemplatePolicyRule>,
}

impl PolicyEnforcer {
    pub fn new() -> PolicyEnforcer {
        PolicyEnforcer {
            acl_enabled: true,
            default_permission: Permission::Deny,
            subject_store: SubjectStore::default(),
            policy_map: PolicyMap::default(),
            template_rules: Vec::new(),
            interface_enabled: InterfaceEnabled::default(),
        }
    }

    /*
       initializes the policy_enforcer
    */
    pub fn init(&mut self, acl_config: &AclConfig) -> ZResult<()> {
        let mut_acl_config = acl_config.clone();
        self.acl_enabled = mut_acl_config.enabled;
        self.default_permission = mut_acl_config.default_permission;
        if self.acl_enabled {
            if let (Some(mut rules), Some(mut subjects), Some(policies)) = (
                mut_acl_config.rules,
                mut_acl_config.subjects,
                mut_acl_config.policies,
            ) {
                if rules.is_empty() || subjects.is_empty() || policies.is_empty() {
                    rules.is_empty().then(|| {
                        tracing::warn!("Access control rules list is empty in config file")
                    });
                    subjects.is_empty().then(|| {
                        tracing::warn!("Access control subjects list is empty in config file")
                    });
                    policies.is_empty().then(|| {
                        tracing::warn!("Access control policies list is empty in config file")
                    });
                    self.policy_map = PolicyMap::default();
                    self.subject_store = SubjectStore::default();
                    if self.default_permission == Permission::Deny {
                        self.interface_enabled = InterfaceEnabled {
                            ingress: true,
                            egress: true,
                        };
                    }
                } else {
                    // check for undefined values in rules and initialize them to defaults
                    for rule in rules.iter_mut() {
                        if rule.id.trim().is_empty() {
                            bail!("Found empty rule id in rules list");
                        }
                        if rule.key_exprs.is_none() && rule.key_expr_templates.is_none() {
                            bail!(
                                "Rule '{}' must define at least one of 'key_exprs' and 'key_expr_templates'",
                                rule.id
                            );
                        }
                        // probe-expand templates so malformed templates fail at config
                        // time rather than silently at transport establishment
                        if let Some(templates) = &rule.key_expr_templates {
                            let probe = TemplateVars {
                                cert_common_name: Some("probe".to_string()),
                                username: Some("probe".to_string()),
                            };
                            for template in templates.iter() {
                                expand_template(template, &probe).map_err(|e| {
                                    zerror!(
                                        "Rule '{}' has invalid key expression template {:?}: {}",
                                        rule.id,
                                        template,
                                        e
                                    )
                                })?;
                            }
                        }
                        if rule.flows.is_none() {
                            tracing::warn!("Rule '{}' flows list is not set. Setting it to both Ingress and Egress", rule.id);
                            rule.flows = Some(
                                [InterceptorFlow::Ingress, InterceptorFlow::Egress]
                                    .to_vec()
                                    .try_into()
                                    .unwrap(),
                            );
                        }
                    }
                    // check for undefined values in subjects and initialize them to defaults
                    for subject in subjects.iter_mut() {
                        if subject.id.trim().is_empty() {
                            bail!("Found empty subject id in subjects list");
                        }
                    }
                    let policy_information =
                        self.policy_information_point(subjects, rules, policies)?;

                    let mut main_policy: PolicyMap = PolicyMap::default();
                    for rule in policy_information.policy_rules {
                        let subject_policy = main_policy.entry(rule.subject_id).or_default();
                        subject_policy
                            .flow_mut(rule.flow)
                            .action_mut(rule.message)
                            .permission_mut(rule.permission)
                            .insert(&rule.key_expr, true);

                        if self.default_permission == Permission::Deny {
                            self.interface_enabled = InterfaceEnabled {
                                ingress: true,
                                egress: true,
                            };
                        } else {
                            match rule.flow {
                                InterceptorFlow::Ingress => {
                                    self.interface_enabled.ingress = true;
                                }
                                InterceptorFlow::Egress => {
                                    self.interface_enabled.egress = true;
                                }
                            }
                        }
                    }
                    // Template rules are not expanded into the policy map (their key
                    // expressions are only known per-connection), but they must still
                    // enable the interceptor interfaces they target.
                    for rule in &policy_information.template_rules {
                        if self.default_permission == Permission::Deny {
                            self.interface_enabled = InterfaceEnabled {
                                ingress: true,
                                egress: true,
                            };
                        } else {
                            match rule.flow {
                                InterceptorFlow::Ingress => {
                                    self.interface_enabled.ingress = true;
                                }
                                InterceptorFlow::Egress => {
                                    self.interface_enabled.egress = true;
                                }
                            }
                        }
                    }
                    self.policy_map = main_policy;
                    self.template_rules = policy_information.template_rules;
                    self.subject_store = policy_information.subject_map;
                }
            } else {
                bail!("All ACL rules/subjects/policies config lists must be provided");
            }
        }
        Ok(())
    }

    /*
       converts the sets of rules from config format into individual rules for each subject, key-expr, action, permission
    */
    pub fn policy_information_point(
        &self,
        subjects: Vec<AclConfigSubjects>,
        rules: Vec<AclConfigRule>,
        policies: Vec<AclConfigPolicyEntry>,
    ) -> ZResult<PolicyInformation> {
        let mut policy_rules: Vec<PolicyRule> = Vec::new();
        let mut template_rules: Vec<TemplatePolicyRule> = Vec::new();
        let mut rule_map = HashMap::new();
        let mut subject_id_map = HashMap::<String, Vec<usize>>::new();
        let mut policy_id_set = HashSet::<String>::new();
        let mut subject_map_builder = SubjectMapBuilder::new();

        // validate rules config and insert them in hashmaps
        for config_rule in rules {
            if rule_map.contains_key(&config_rule.id) {
                bail!(
                    "Rule id must be unique: id '{}' is repeated",
                    config_rule.id
                );
            }

            rule_map.insert(config_rule.id.clone(), config_rule);
        }

        for config_subject in subjects.into_iter() {
            if subject_id_map.contains_key(&config_subject.id) {
                bail!(
                    "Subject id must be unique: id '{}' is repeated",
                    config_subject.id
                );
            }
            // validate subject config fields
            if config_subject
                .interfaces
                .as_ref()
                .is_some_and(|interfaces| interfaces.iter().any(|face| face.0.trim().is_empty()))
            {
                bail!(
                    "Found empty interface value in subject '{}'",
                    config_subject.id
                );
            }
            if config_subject
                .cert_common_names
                .as_ref()
                .is_some_and(|cert_common_names| {
                    cert_common_names.iter().any(|ccn| ccn.0.trim().is_empty())
                })
            {
                bail!(
                    "Found empty cert_common_name value in subject '{}'",
                    config_subject.id
                );
            }
            if config_subject
                .cert_common_name_prefixes
                .as_ref()
                .is_some_and(|prefixes| prefixes.iter().any(|ccn| ccn.0.trim().is_empty()))
            {
                bail!(
                    "Found empty cert_common_name_prefixes value in subject '{}'",
                    config_subject.id
                );
            }
            if config_subject.usernames.as_ref().is_some_and(|usernames| {
                usernames
                    .iter()
                    .any(|username| username.0.trim().is_empty())
            }) {
                bail!(
                    "Found empty username value in subject '{}'",
                    config_subject.id
                );
            }
            // Map properties to SubjectProperty type
            // FIXME: Unnecessary .collect() because of different iterator types
            let interfaces = config_subject
                .interfaces
                .map(|interfaces| {
                    interfaces
                        .into_iter()
                        .map(SubjectProperty::Exactly)
                        .collect::<Vec<_>>()
                })
                .unwrap_or(vec![SubjectProperty::Wildcard]);
            // exact matches and prefix matches are combined into a single OR-list
            let mut cert_common_names: Vec<SubjectProperty<CertCommonName>> = config_subject
                .cert_common_names
                .map(|cert_common_names| {
                    cert_common_names
                        .into_iter()
                        .map(SubjectProperty::Exactly)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if let Some(prefixes) = config_subject.cert_common_name_prefixes {
                cert_common_names.extend(prefixes.into_iter().map(SubjectProperty::Prefix));
            }
            if cert_common_names.is_empty() {
                cert_common_names.push(SubjectProperty::Wildcard);
            }
            // FIXME: Unnecessary .collect() because of different iterator types
            let usernames = config_subject
                .usernames
                .map(|usernames| {
                    usernames
                        .into_iter()
                        .map(SubjectProperty::Exactly)
                        .collect::<Vec<_>>()
                })
                .unwrap_or(vec![SubjectProperty::Wildcard]);
            // FIXME: Unnecessary .collect() because of different iterator types
            let link_types = config_subject
                .link_protocols
                .map(|link_types| {
                    link_types
                        .into_iter()
                        .map(SubjectProperty::Exactly)
                        .collect::<Vec<_>>()
                })
                .unwrap_or(vec![SubjectProperty::Wildcard]);
            // FIXME: Unnecessary .collect() because of different iterator types
            let zids = config_subject
                .zids
                .map(|zids| {
                    zids.into_iter()
                        .map(SubjectProperty::Exactly)
                        .collect::<Vec<_>>()
                })
                .unwrap_or(vec![SubjectProperty::Wildcard]);

            // create ACL subject combinations
            let subject_combination_ids = interfaces
                .into_iter()
                .cartesian_product(cert_common_names)
                .cartesian_product(usernames)
                .cartesian_product(link_types)
                .cartesian_product(zids)
                .map(
                    |((((interface, cert_common_name), username), link_type), zid)| {
                        let subject = Subject {
                            interface,
                            cert_common_name,
                            username,
                            link_type,
                            zid,
                        };
                        subject_map_builder.insert_or_get(subject)
                    },
                )
                .collect();
            subject_id_map.insert(config_subject.id.clone(), subject_combination_ids);
        }
        // finally, handle policy content
        for (entry_id, entry) in policies.iter().enumerate() {
            if let Some(policy_custom_id) = &entry.id {
                if !policy_id_set.insert(policy_custom_id.clone()) {
                    bail!(
                        "Policy id must be unique: id '{}' is repeated",
                        policy_custom_id
                    );
                }
            }
            // validate policy config lists
            if entry.rules.is_empty() || entry.subjects.is_empty() {
                bail!(
                    "Policy #{} is malformed: empty subjects or rules list",
                    entry_id
                );
            }
            for subject_config_id in &entry.subjects {
                if subject_config_id.trim().is_empty() {
                    bail!("Found empty subject id in policy #{}", entry_id)
                }
                if !subject_id_map.contains_key(subject_config_id) {
                    bail!(
                        "Subject '{}' in policy #{} does not exist in subjects list",
                        subject_config_id,
                        entry_id
                    )
                }
            }
            // Create PolicyRules
            for rule_id in &entry.rules {
                if rule_id.trim().is_empty() {
                    bail!("Found empty rule id in policy #{}", entry_id)
                }
                let rule = rule_map.get(rule_id).ok_or(zerror!(
                    "Rule '{}' in policy #{} does not exist in rules list",
                    rule_id,
                    entry_id
                ))?;
                for subject_config_id in &entry.subjects {
                    let subject_combination_ids = subject_id_map
                        .get(subject_config_id)
                        .expect("config subject id should exist in subject_id_map");
                    for subject_id in subject_combination_ids {
                        for flow in rule
                            .flows
                            .as_ref()
                            .expect("flows list should be defined in rule")
                        {
                            for message in &rule.messages {
                                if let Some(key_exprs) = &rule.key_exprs {
                                    for key_expr in key_exprs.iter() {
                                        policy_rules.push(PolicyRule {
                                            subject_id: *subject_id,
                                            key_expr: key_expr.clone(),
                                            message: *message,
                                            permission: rule.permission,
                                            flow: *flow,
                                        });
                                    }
                                }
                                if let Some(templates) = &rule.key_expr_templates {
                                    for template in templates.iter() {
                                        template_rules.push(TemplatePolicyRule {
                                            subject_id: *subject_id,
                                            template: template.clone(),
                                            message: *message,
                                            permission: rule.permission,
                                            flow: *flow,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(PolicyInformation {
            subject_map: subject_map_builder.build(),
            policy_rules,
            template_rules,
        })
    }

    /// Builds the per-connection policies for a transport, expanding template rules with
    /// the identity attributes authenticated on that transport. `subject_vars` pairs
    /// each matched subject id with the identity attributes of the transport link(s)
    /// that satisfied its subject config. The result maps each subject id to the policy
    /// expanded for it, so that these policies can be evaluated as part of their subject
    /// (a subject's effective policy is the union of its static rules and its expanded
    /// template rules). Returns `None` when no template rule applies to this transport.
    pub(crate) fn build_connection_policies(
        &self,
        subject_vars: &[(usize, TemplateVars)],
    ) -> Option<HashMap<usize, FlowPolicy>> {
        if self.template_rules.is_empty() || subject_vars.is_empty() {
            return None;
        }
        let mut policies: HashMap<usize, FlowPolicy> = HashMap::new();
        for rule in &self.template_rules {
            for (subject_id, vars) in subject_vars
                .iter()
                .filter(|(subject_id, _)| *subject_id == rule.subject_id)
            {
                match expand_template(&rule.template, vars) {
                    Ok(key_expr) => {
                        policies
                            .entry(*subject_id)
                            .or_default()
                            .flow_mut(rule.flow)
                            .action_mut(rule.message)
                            .permission_mut(rule.permission)
                            .insert(&key_expr, true);
                    }
                    Err(e) => {
                        // the rule is skipped: the connection falls back to whatever the
                        // static rules and default permission decide
                        tracing::warn!(
                            "ACL: skipping template rule {:?} for connection identity {:?}: {}",
                            rule.template,
                            vars,
                            e
                        );
                    }
                }
            }
        }
        (!policies.is_empty()).then_some(policies)
    }

    /// Decision for a single subject: its effective policy is the union of its static
    /// rules (`policy_map`) and its per-connection expanded template rules (`conn_policy`).
    /// Precedence: explicit deny > explicit allow > default permission. A subject with
    /// no rules at all yields the default permission.
    pub(crate) fn subject_decision(
        &self,
        subject: usize,
        conn_policy: Option<&FlowPolicy>,
        flow: InterceptorFlow,
        message: AclMessage,
        key_expr: &keyexpr,
    ) -> Permission {
        let static_policy = self.policy_map.get(&subject);
        let hit = |policy: &FlowPolicy, permission: Permission| {
            policy
                .flow(flow)
                .action(message)
                .permission(permission)
                .nodes_including(key_expr)
                .any(|n| n.weight().is_some())
        };
        if static_policy.is_some_and(|p| hit(p, Permission::Deny))
            || conn_policy.is_some_and(|p| hit(p, Permission::Deny))
        {
            return Permission::Deny;
        }
        if self.default_permission == Permission::Allow {
            return Permission::Allow;
        }
        if static_policy.is_some_and(|p| hit(p, Permission::Allow))
            || conn_policy.is_some_and(|p| hit(p, Permission::Allow))
        {
            Permission::Allow
        } else {
            Permission::Deny
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(cert_common_name: Option<&str>, username: Option<&str>) -> TemplateVars {
        TemplateVars {
            cert_common_name: cert_common_name.map(str::to_string),
            username: username.map(str::to_string),
        }
    }

    #[test]
    fn test_expand_template() {
        let cn = vars(Some("t1"), None);
        let ke = expand_template("tenant/${cert_common_name}/**", &cn).unwrap();
        assert_eq!(ke.as_str(), "tenant/t1/**");
        // placeholder inside a chunk
        let ke = expand_template("ns_${cert_common_name}/**", &cn).unwrap();
        assert_eq!(ke.as_str(), "ns_t1/**");
        // multiple occurrences
        let ke = expand_template("${cert_common_name}/sub/${cert_common_name}", &cn).unwrap();
        assert_eq!(ke.as_str(), "t1/sub/t1");
        // no placeholder: template is used as-is
        let ke = expand_template("static/key", &cn).unwrap();
        assert_eq!(ke.as_str(), "static/key");
        // username placeholder
        let ke = expand_template("tenant/${username}/**", &vars(None, Some("u1"))).unwrap();
        assert_eq!(ke.as_str(), "tenant/u1/**");
        // both placeholders in one template
        let ke = expand_template(
            "${cert_common_name}/${username}/**",
            &vars(Some("t1"), Some("u1")),
        )
        .unwrap();
        assert_eq!(ke.as_str(), "t1/u1/**");
    }

    #[test]
    fn test_expand_template_requires_matching_identity() {
        // template references an identity the connection does not have
        assert!(expand_template("tenant/${cert_common_name}/**", &vars(None, Some("u1"))).is_err());
        assert!(expand_template("tenant/${username}/**", &vars(Some("t1"), None)).is_err());
        // no placeholder: no identity required
        assert!(expand_template("static/key", &vars(None, None)).is_ok());
        // unknown placeholders are not substituted, and are rejected by key expression
        // validation ('$' is reserved), so typos fail at config time via probe expansion
        assert!(expand_template("tenant/${unknown}/**", &vars(Some("t1"), Some("u1"))).is_err());
    }

    #[test]
    fn test_expand_template_rejects_unsafe_substitutions() {
        for value in ["", "*", "**", "a/b", "a*", "$*", "a?b", "a#b"] {
            assert!(
                expand_template("tenant/${cert_common_name}/**", &vars(Some(value), None)).is_err(),
                "common name {value:?} should be rejected"
            );
            assert!(
                expand_template("tenant/${username}/**", &vars(None, Some(value))).is_err(),
                "username {value:?} should be rejected"
            );
        }
    }

    #[test]
    fn test_subject_property_prefix_matches() {
        let prefix = SubjectProperty::Prefix(CertCommonName("t".to_string()));
        assert!(prefix.matches(Some(&CertCommonName("t1".to_string()))));
        assert!(prefix.matches(Some(&CertCommonName("t".to_string()))));
        assert!(!prefix.matches(Some(&CertCommonName("x1".to_string()))));
        assert!(!prefix.matches(None));
        // prefix matching is only supported for certificate common names
        let prefix = SubjectProperty::Prefix(Username("user".to_string()));
        assert!(!prefix.matches(Some(&Username("user1".to_string()))));
    }
}
