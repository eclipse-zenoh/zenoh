impl PolicyEnforcer {
    pub fn new() -> ZResult<PolicyEnforcer> {
        // PolicyEnforcer
        Ok(PolicyEnforcer(0, Vec::new()))
    }

    pub fn init(&mut self) -> ZResult<()> {
        /*
           Initializes the policy for the control logic
           loads policy into memory from file/network path
           creates the policy hashmap with the ke-tries for ke matching
           can have policy-type in the mix here...need to verify
        */
        //set the policy type to 1,2,3 depending on the user input in the "rules" file

        let (rule_type, rule_set) = self.policy_resource_point("rules_test_thr.json5").unwrap();

        self.build_policy_map(rule_type, rule_set)
            .expect("policy not established");

        /* setup a temporary variable here to hold all the values */
        //also should start the logger here
        Ok(())
    }

    pub fn build_policy_map(&mut self, rule_type: u8, rule_set: Vec<PolicyRule>) -> ZResult<()> {
        //convert vector of rules to a hashmap mapping sub-act to ketrie
        /*
            representaiton of policy list as a vector of hashmap of trees
            each hashmap maps a subject (ID/atttribute) to a trie of allowed values
            using fr-trie for now as a placeholder for key-matching
        */
        self.0 = rule_type;
        let policy_type = &mut self.1;
        policy_type.push(Self::build_id_map(rule_set));

        /*
        match rule_type {
            1 => policy_type.push(Self::build_id_map(rule_set)),

            2 => policy_type.push(Self::build_attribute_map(rule_set)),
            3 | 4 => {
                policy_type.push(Self::build_id_map(rule_set.clone()));
                policy_type.push(Self::build_attribute_map(rule_set));
            }
            _ => bail!("bad entry for type"),
        }
        */
        Ok(())
    }

    pub fn build_id_map(rule_set: Vec<PolicyRule>) -> SubActPolicy {
        let mut policy: SubActPolicy = FxHashMap::default();
        for v in rule_set {
            //  for now permission being false means this ke will not be inserted into the trie of allowed ke's
            let perm = v.permission;
            if !perm {
                continue;
            }
            let sub = v.sub;
            let ke = v.ke;
            let action_flag = match v.action {
                Action::Read => ActionFlag::Read,
                Action::Write => ActionFlag::Write,
                Action::None => ActionFlag::None,
                Action::DeclareSub => ActionFlag::DeclareSub,
                Action::Delete => ActionFlag::Delete,
                Action::DeclareQuery => ActionFlag::DeclareQuery,
            };
            let subact = SubAct(sub, action_flag.clone());
            //match subject to the policy hashmap
            #[allow(clippy::map_entry)]
            if !policy.contains_key(&subact) {
                //create new entry for subject + ke-tree
                let mut ketree = KeTreeFast::new();
                ketree.insert(keyexpr::new(&ke).unwrap(), action_flag);
                policy.insert(subact, ketree);
            } else {
                let ketree = policy.get_mut(&subact).unwrap();
                ketree.insert(keyexpr::new(&ke).unwrap(), action_flag);
            }
        }
        policy
    }

    pub fn policy_enforcement_point(
        &self,
        reuqest_info: RequestInfo,
        action: ActionFlag,
    ) -> ZResult<bool> {
        /*
           input: new_context and action (sub,act for now but will need attribute values later)
           output: allow/denyca
           function: depending on the msg, builds the subject, builds the request, passes the request to policy_decision_point()
                    collects result from PDP and then uses that allow/deny output to block or pass the msg to routing table
        */
        let ke = reuqest_info.ke;
        let zid = reuqest_info.zid;
        let _attribute = reuqest_info.attributes;

        let subject = PolicySubject::Id(zid);
        let request = Request {
            sub: subject,
            obj: ke.to_string(),
            action,
        };
        let decision = self.policy_decision_point(request);
        Ok(decision)
    }

    pub fn policy_decision_point(&self, request: Request) -> bool {
        /*
            input: (request)
            output: true(allow)/false(deny)
            function: process the request received from PEP against the policy (self)
                    the policy list is chosen based on the policy-type specified in the rules file
                    policy list is be a hashmap of subject->ketries (test and discuss)
        */
        //compare the request to the vec of values...matching depends on the value of the policy type
        return self.matching_algo(0, request);

        // false
    }

    pub fn policy_information_point<'a>(&self, file_path: &'a str) -> ZResult<PolicyInformation> {
        let policy_definition = "userid and nettype"; //this will be the value deciding how matcher functions are called
        let mut policy_rules: Vec<AttributeRules> = Vec::new();
        let userid_attr = AttributeRules {
            attribute_name: "userid".to_owned(),
            attribute_rules: [
                SinglePolicyRule {
                    sub: "aaa3b411006ad57868988f9fec672a31".to_string(),
                    ke: "test/thr".to_string(),
                    action: Action::Write,
                    permission: true,
                },
                SinglePolicyRule {
                    sub: "bbb3b411006ad57868988f9fec672a31".to_string(),
                    ke: "test/thr".to_string(),
                    action: Action::Read,
                    permission: true,
                },
                SinglePolicyRule {
                    sub: "aaabbb11006ad57868988f9fec672a31".to_string(),
                    ke: "test/thr".to_string(),
                    action: Action::Read,
                    permission: true,
                },
                SinglePolicyRule {
                    sub: "aaabbb11006ad57868988f9fec672a31".to_string(),
                    ke: "test/thr".to_string(),
                    action: Action::Write,
                    permission: true,
                },
            ]
            .to_vec(),
        };
        let nettype_attr = AttributeRules {
            attribute_name: "networktype".to_owned(),
            attribute_rules: [
                SinglePolicyRule {
                    sub: "wifi".to_string(),
                    ke: "test/thr".to_string(),
                    action: Action::Write,
                    permission: true,
                },
                SinglePolicyRule {
                    sub: "wifi".to_string(),
                    ke: "test/thr".to_string(),
                    action: Action::Read,
                    permission: true,
                },
            ]
            .to_vec(),
        };
        let location_attr = AttributeRules {
            attribute_name: "location".to_owned(),
            attribute_rules: [
                SinglePolicyRule {
                    sub: "location_1".to_string(),
                    ke: "test/thr".to_string(),
                    action: Action::Write,
                    permission: true,
                },
                SinglePolicyRule {
                    sub: "location_2".to_string(),
                    ke: "test/thr".to_string(),
                    action: Action::Read,
                    permission: true,
                },
            ]
            .to_vec(),
        };

        /*
        for example, if policy_defintiion = "userid and nettype"
        our request will be 2 different calls to pdp with different rewuest values
        so we will get val1= matcher_function(sub=userid...), val2=matcher_function(sub=nettype...)
        and our policy function will be val1 and val2 (from "userid and nettype" given in the policy_defintiion)
        and matcher function be mathcer_function(request)
        in our pdp, we will call matcher("subval")
         */
        /*
        get values from json str??
         */
        policy_rules.push(userid_attr);
        policy_rules.push(nettype_attr);
        policy_rules.push(location_attr);

        let list_of_attributes = vec![
            "userid".to_string(),
            "networktype".to_string(),
            "location".to_string(),
        ];
        let userid_enabled = true;
        let networktype_enabled = true;

        Ok(PolicyInformation {
            policydefition: policy_definition.to_owned(),
            userid_enabled: true,
            networktype_enabled: true,
            attribute_list: list_of_attributes,
            policy_rules,
        })
    }

    pub fn policy_resource_point(&self, file_path: &str) -> ZResult<(u8, Vec<PolicyRule>)> {
        /*
           input: path to rules.json file
           output: loads the appropriate policy into the memory and returns back a vector of rules and the policy type as specified in the file;
           * might also be the point to select AC type (ACL, ABAC etc)?? *
        */
        let policytype: u8 = 1;
        let _policy_string = [
            "userid",
            "attribute",
            "userid or attribute",
            "userid and attribute",
        ]; //if it is
           //match config value against it
           //if it doesn't match, abort. else use that index henceforth

        let vec_ids = [
            "aaa3b411006ad57868988f9fec672a31",
            "bbb3b411006ad57868988f9fec672a31",
            "aaabbb11006ad57868988f9fec672a31",
            "aaabbb11006ad57868988f9fec672a31",
        ];
        let vec_actions: Vec<Action> =
            vec![Action::Write, Action::Read, Action::Write, Action::Read];
        let mut policyrules: Vec<PolicyRule> = Vec::new();

        let wc_keys: [&str; 16] = [
            "test/demo/**/example/**",
            "test/demo/example",
            "test/demo/example",
            "test/*/example",
            "**/example",
            "**/e",
            "e/demo/example",
            "**/demo/example",
            "**/@example/**",
            "**",
            "**/@example",
            "@example/**",
            "@example/a",
            "test/@example",
            "demo/a$*a/demo/bb",
            "**/b$*/ee",
        ];

        let no_wc_keys: [&str; 100] = [
            "test/example/a/b",
            "test/b/example/a/bone/activity/basin",
            "some/demo/test/a",
            "some/birth/example/a/bone/bit/airplane",
            "some/demo/test/a/blood",
            "test/example/a",
            "test/authority/example/a/acoustics/board",
            "some/b/example/a",
            "some/room/example/net",
            "test/example/a/ants/humidity",
            "some/account/example/a/argument/humidity",
            "test/b/example/a/info/birthday",
            "test/b/example/org/info/humidity",
            "some/info/example/net",
            "some/demo/test/a",
            "test/amusement/example/a/angle",
            "some/b/example/a/baseball",
            "test/b/example/org",
            "some/b/example/a",
            "test/b/example/a/basket/humidity",
            "test/example/a",
            "test/b/example/net/army/aunt/d",
            "some/appliance/example/net/box",
            "some/b/example/org/number",
            "some/example/net/beginner/d",
            "some/birthday/example/net",
            "test/believe/example/a/battle",
            "test/b/example/org/baseball/speedb",
            "some/basket/example/a",
            "some/b/example/net/birds",
            "some/demo/test/a",
            "test/bear/example/a/blow",
            "test/b/example/net",
            "some/demo/test/a/achiever/action",
            "test/b/example/net",
            "test/b/example/a",
            "test/b/example/a/believe/temp",
            "test/example/a/basketball",
            "test/example/a/afternoon/d/bells",
            "test/example/a/bubble/brick",
            "test/b/example/a",
            "test/boot/example/org/boat/board",
            "test/b/example/a",
            "test/room/example/a/c/d",
            "some/b/example/org",
            "some/b/example/a/box/book/temp",
            "some/b/example/a/adjustment/temp",
            "some/example/net/belief/afternoon",
            "test/b/example/a/activity/info",
            "some/b/example/org/sensor/arm",
            "some/zenoh/example/org/bead/bridge",
            "test/brother/example/a/bath",
            "test/example/a",
            "test/example/a/sensor",
            "some/back/example/a/balance/bird/humidity",
            "test/zenoh/example/a/box/action/humidity",
            "test/b/example/a",
            "some/demo/test/a/bedroom/temp",
            "some/b/example/a/ball/humidity",
            "test/airplane/example/a/art/animal",
            "some/example/net",
            "test/b/example/a",
            "some/demo/test/a/baseball/achiever",
            "some/demo/test/a/berry/arch/temp",
            "test/arithmetic/example/a/basket",
            "some/example/net/art/bikes/humidity",
            "some/demo/test/a/bedroom",
            "some/demo/test/a",
            "some/appliance/example/a",
            "test/b/example/a",
            "test/b/example/a/agreement",
            "some/example/net/bird/sound",
            "test/b/example/a/argument/info/basket",
            "some/b/example/a/balance/boundary",
            "some/arch/example/a/argument",
            "some/demo/test/a/zenoh/brake",
            "test/b/example/a/bath/brass",
            "some/anger/example/net",
            "test/b/example/a/boat/humidity",
            "some/demo/test/a/b/c",
            "test/b/example/a/brother/temp",
            "test/b/example/a",
            "some/b/example/a",
            "test/b/example/org",
            "some/b/example/a/amount/b",
            "some/b/example/org/heat/humidity",
            "some/demo/test/a",
            "some/b/example/edu/activity",
            "some/argument/example/a/suggest/humidity",
            "test/example/a/believe/anger/humidity",
            "test/b/example/a/sensor/b/c",
            "test/example/edu/agreement",
            "test/example/org",
            "some/demo/test/a",
            "test/b/example/a/airplane/wing",
            "test/b/example/a",
            "some/b/example/net/beef/bedroom/temp",
            "test/b/example/a/blade/angle",
            "some/b/example/c/d",
            "test/b/example/a",
        ];

        //valid one that works
        for i in 0..4 {
            policyrules.push(PolicyRule {
                sub: PolicySubject::Id(ZenohId::from_str(vec_ids[i]).unwrap()),
                ke: "test/thr".to_string(),
                action: vec_actions.get(i).unwrap().clone(),
                permission: true,
            });
        }

        for i in 0..4 {
            for j in no_wc_keys {
                policyrules.push(PolicyRule {
                    sub: PolicySubject::Id(ZenohId::from_str(vec_ids[i]).unwrap()),
                    ke: j.to_string(),
                    action: vec_actions.get(i).unwrap().clone(),
                    permission: true,
                });
            }
        }

        //list of attributes from the file

        println!("policy rules : {:?}", policyrules);

        //get value from the file
        let file_input = {
            let file_content =
                fs::read_to_string("rules_test_thr.json").expect("error reading file");
            serde_json::from_str::<Value>(&file_content).expect("error serializing to JSON")
        };
        println!("file input {}", file_input);

        Ok((policytype, policyrules))
    }

    fn matching_algo(&self, matching_type: usize, request: Request) -> bool {
        //return true;
        let ke = request.obj;
        let sub = request.sub;
        let action = request.action;
        let subact = SubAct(sub, action);

        match self.1[matching_type].get(&subact) {
            Some(ktrie) => {
                let result = ktrie.nodes_including(keyexpr::new(&ke).unwrap()).count();
                result != 0
            }
            None => false,
        }
    }
}
