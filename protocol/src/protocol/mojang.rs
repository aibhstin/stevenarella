// Copyright 2016 Matthew Collins
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use openssl;
use serde_json;
use serde_json::builder::ObjectBuilder;
use hyper;

#[derive(Clone, Debug)]
pub struct Profile {
    pub username: String,
    pub id: String,
    pub access_token: String,
}

const JOIN_URL: &'static str = "https://sessionserver.mojang.com/session/minecraft/join";
const LOGIN_URL: &'static str = "https://authserver.mojang.com/authenticate";
const REFRESH_URL: &'static str = "https://authserver.mojang.com/refresh";
const VALIDATE_URL: &'static str = "https://authserver.mojang.com/validate";

impl Profile {
    pub fn login(username: &str, password: &str, token: &str) -> Result<Profile, super::Error> {
        let req_msg = ObjectBuilder::new()
                           .insert("username", username)
                           .insert("password", password)
                           .insert("clientToken", token)
                           .insert_object("agent", |b| b
                                .insert("name", "Minecraft")
                                .insert("version", 1)
                            )
                           .unwrap();
        let req = try!(serde_json::to_string(&req_msg));

        let client = hyper::Client::new();
        let res = try!(client.post(LOGIN_URL)
                        .body(&req)
                        .header(hyper::header::ContentType("application/json".parse().unwrap()))
                        .send());

        let ret: serde_json::Value = try!(serde_json::from_reader(res));
        if let Some(error) = ret.find("error").and_then(|v| v.as_string()) {
            return Err(super::Error::Err(format!(
                "{}: {}",
                error,
                ret.find("errorMessage").and_then(|v| v.as_string()).unwrap())
            ));
        }
        Ok(Profile {
            username: ret.lookup("selectedProfile.name").and_then(|v| v.as_string()).unwrap().to_owned(),
            id: ret.lookup("selectedProfile.id").and_then(|v| v.as_string()).unwrap().to_owned(),
            access_token: ret.find("accessToken").and_then(|v| v.as_string()).unwrap().to_owned(),
        })
    }

    pub fn refresh(self, token: &str) -> Result<Profile, super::Error> {
        let req_msg = ObjectBuilder::new()
                           .insert("accessToken", self.access_token.clone())
                           .insert("clientToken", token)
                           .unwrap();
        let req = try!(serde_json::to_string(&req_msg));

        let client = hyper::Client::new();
        let res = try!(client.post(VALIDATE_URL)
                        .body(&req)
                        .header(hyper::header::ContentType("application/json".parse().unwrap()))
                        .send());

        if res.status != hyper::status::StatusCode::NoContent {
            // Refresh needed
            let res = try!(client.post(REFRESH_URL)
                            .body(&req)
                            .header(hyper::header::ContentType("application/json".parse().unwrap()))
                            .send());

            let ret: serde_json::Value = try!(serde_json::from_reader(res));
            if let Some(error) = ret.find("error").and_then(|v| v.as_string()) {
                return Err(super::Error::Err(format!(
                    "{}: {}",
                    error,
                    ret.find("errorMessage").and_then(|v| v.as_string()).unwrap())
                ));
            }
            return Ok(Profile {
                username: ret.lookup("selectedProfile.name").and_then(|v| v.as_string()).unwrap().to_owned(),
                id: ret.lookup("selectedProfile.id").and_then(|v| v.as_string()).unwrap().to_owned(),
                access_token: ret.find("accessToken").and_then(|v| v.as_string()).unwrap().to_owned(),
            });
        }
        Ok(self)
    }

    pub fn join_server(&self, server_id: &str, shared_key: &Vec<u8>, public_key: &Vec<u8>) -> Result<(), super::Error> {
        let mut sha1 = openssl::SHA1::new();
        sha1.update(server_id.as_bytes());
        sha1.update(&shared_key[..]);
        sha1.update(&public_key[..]);
        let mut hash = sha1.bytes();

        // Mojang uses a hex method which allows for
        // negatives so we have to account for that.
        let negative = hash[0] & 0x80 == 0x80;
        if negative {
            twos_compliment(&mut hash);
        }
        let hash_str = hash.iter().map(|b| format!("{:02X}", b)).collect::<Vec<String>>().join("");
        let hash_val = hash_str.trim_matches('0');
        let hash_str = if negative {
            "-".to_owned() + &hash_val[..]
        } else {
            hash_val.to_owned()
        };

        let join_msg = ObjectBuilder::new()
                           .insert("accessToken", &self.access_token)
                           .insert("selectedProfile", &self.id)
                           .insert("serverId", hash_str)
                           .unwrap();
        let join = serde_json::to_string(&join_msg).unwrap();

        let client = hyper::Client::new();
        let res = try!(client.post(JOIN_URL)
                        .body(&join)
                        .header(hyper::header::ContentType("application/json".parse().unwrap()))
                        .send());

        if res.status == hyper::status::StatusCode::NoContent {
            Ok(())
        } else {
            Err(super::Error::Err("Failed to auth with server".to_owned()))
        }
    }

    pub fn is_complete(&self) -> bool {
        !self.username.is_empty() && !self.id.is_empty() && !self.access_token.is_empty()
    }
}

fn twos_compliment(data: &mut Vec<u8>) {
    let mut carry = true;
    for i in (0..data.len()).rev() {
        data[i] = !data[i];
        if carry {
            carry = data[i] == 0xFF;
            data[i] += 1;
        }
    }
}
