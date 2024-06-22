// Copyright 2018 Brandon W Maister
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::env;

use git_instafix::load_config_from_args_env_git;

fn main() {
    let config = load_config_from_args_env_git();

    if config.help_themes {
        git_instafix::print_themes();
        return;
    }

    if let Err(e) = git_instafix::instafix(config) {
        // An empty message means don't display any error message
        let msg = e.to_string();
        if !msg.is_empty() {
            if env::var("RUST_BACKTRACE").as_deref() == Ok("1") {
                println!("Error: {:?}", e);
            } else {
                println!("Error: {:#}", e);
            }
        }
        std::process::exit(1);
    }
}
