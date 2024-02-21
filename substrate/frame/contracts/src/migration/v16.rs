// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

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

//! Version 16 does not have any storage update. Instead, it serves as an indicator for the
//! stabilization of the following APIs: `call_v2`, `instantiate_v2`, `lock_delegate_dependency`,
//! and `unlock_delegate_dependency`.

const VERSION: u16 = 16;
pub type Migration = crate::NoopMigration<VERSION>;
