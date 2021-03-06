// Copyright (c) 2013-2015 Sandstorm Development Group, Inc. and contributors
// Licensed under the MIT License:
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

use std;
use std::vec::Vec;
use any_pointer;
use private::capability::ClientHook;
use private::units::*;
use private::arena::{BuilderArena, ReaderArena, SegmentBuilder, SegmentReader, NumWords, ZeroedWords};
use private::layout;
use traits::{FromPointerReader, FromPointerBuilder, SetPointerBuilder};
use Word;

#[derive(Copy)]
pub struct ReaderOptions {
    pub traversal_limit_in_words : u64,
    pub nesting_limit : i32,

    /// If true, malformed messages trigger task failure.
    /// If false, malformed messages fall back to default values.
    pub fail_fast : bool,
}

pub const DEFAULT_READER_OPTIONS : ReaderOptions =
    ReaderOptions { traversal_limit_in_words : 8 * 1024 * 1024, nesting_limit : 64,
                    fail_fast : true };

impl ReaderOptions {
    pub fn new() -> ReaderOptions { DEFAULT_READER_OPTIONS }

    pub fn nesting_limit<'a>(&'a mut self, value : i32) -> &'a mut ReaderOptions {
        self.nesting_limit = value;
        return self;
    }

    pub fn traversal_limit_in_words<'a>(&'a mut self, value : u64) -> &'a mut ReaderOptions {
        self.traversal_limit_in_words = value;
        return self;
    }

    pub fn fail_fast<'a>(&'a mut self, value : bool) -> &'a mut ReaderOptions {
        self.fail_fast = value;
        return self;
    }
}


type SegmentId = u32;

pub trait MessageReader {
    fn get_segment(&self, id : usize) -> &[Word];
    fn arena(&self) -> &ReaderArena;
    fn mut_arena(&mut self) -> &mut ReaderArena;
    fn get_options(&self) -> &ReaderOptions;

    fn get_root_internal(&self) -> any_pointer::Reader {
        unsafe {
            let segment : *const SegmentReader = &self.arena().segment0;

            let pointer_reader = layout::PointerReader::get_root(
                segment, (*segment).get_start_ptr(), self.get_options().nesting_limit);

            any_pointer::Reader::new(pointer_reader)
        }
    }

    fn get_root<'a, T : FromPointerReader<'a>>(&'a self) -> T {
        self.get_root_internal().get_as()
    }

    fn init_cap_table(&mut self, cap_table : Vec<Option<Box<ClientHook+Send>>>) {
        self.mut_arena().init_cap_table(cap_table);
    }
}

pub struct SegmentArrayMessageReader<'a> {
    segments : &'a [ &'a [Word]],
    options : ReaderOptions,
    arena : Box<ReaderArena>
}


impl <'a> MessageReader for SegmentArrayMessageReader<'a> {
    fn get_segment<'b>(&'b self, id : usize) -> &'b [Word] {
        self.segments[id]
    }

    fn arena<'b>(&'b self) -> &'b ReaderArena { &*self.arena }
    fn mut_arena<'b>(&'b mut self) -> &'b mut ReaderArena { &mut *self.arena }

    fn get_options<'b>(&'b self) -> &'b ReaderOptions {
        return &self.options;
    }
}

impl <'a> SegmentArrayMessageReader<'a> {

    pub fn new<'b>(segments : &'b [&'b [Word]], options : ReaderOptions) -> SegmentArrayMessageReader<'b> {
        assert!(segments.len() > 0);
        SegmentArrayMessageReader {
            segments : segments,
            arena : ReaderArena::new(segments, options),
            options : options
        }
    }
}

#[derive(Copy)]
pub enum AllocationStrategy {
    FixedSize,
    GrowHeuristically
}

pub const SUGGESTED_FIRST_SEGMENT_WORDS : u32 = 1024;
pub const SUGGESTED_ALLOCATION_STRATEGY : AllocationStrategy = AllocationStrategy::GrowHeuristically;

#[derive(Copy)]
pub struct BuilderOptions {
    pub first_segment_words : u32,
    pub allocation_strategy : AllocationStrategy,

    /// If true, malformed messages trigger task failure.
    /// If false, malformed messages fall back to default values.
    pub fail_fast : bool,
}

impl BuilderOptions {
    pub fn new() -> BuilderOptions {
        BuilderOptions {first_segment_words : SUGGESTED_FIRST_SEGMENT_WORDS,
                        allocation_strategy : AllocationStrategy::GrowHeuristically,
                        fail_fast : true }
    }

    pub fn first_segment_words<'a>(&'a mut self, value : u32) -> &'a mut BuilderOptions {
        self.first_segment_words = value;
        return self;
    }

    pub fn allocation_strategy<'a>(&'a mut self, value : AllocationStrategy) -> &'a mut BuilderOptions {
        self.allocation_strategy = value;
        return self;
    }

    pub fn fail_fast<'a>(&'a mut self, value : bool) -> &'a mut BuilderOptions {
        self.fail_fast = value;
        return self;
    }
}


pub trait MessageBuilder {
    fn mut_arena(&mut self) -> &mut BuilderArena;
    fn arena(&self) -> &BuilderArena;


    // XXX is there a way to make this private?
    fn get_root_internal<'a>(&mut self) -> any_pointer::Builder<'a> {
        let root_segment = &mut self.mut_arena().segment0 as *mut SegmentBuilder;

        if self.arena().segment0.current_size() == 0 {
            match self.mut_arena().segment0.allocate(WORDS_PER_POINTER as u32) {
                None => {panic!("could not allocate root pointer") }
                Some(location) => {
                    assert!(location == self.arena().segment0.get_ptr_unchecked(0),
                            "First allocated word of new segment was not at offset 0");

                    any_pointer::Builder::new(layout::PointerBuilder::get_root(root_segment, location))

                }
            }
        } else {
            any_pointer::Builder::new(
                layout::PointerBuilder::get_root(root_segment,
                                                 self.arena().segment0.get_ptr_unchecked(0)))
        }

    }

    fn init_root<'a, T : FromPointerBuilder<'a>>(&'a mut self) -> T {
        self.get_root_internal().init_as()
    }

    fn get_root<'a, T : FromPointerBuilder<'a>>(&'a mut self) -> T {
        self.get_root_internal().get_as()
    }

    fn set_root<To, From : SetPointerBuilder<To>>(&mut self, value : From) {
        self.get_root_internal().set_as(value);
    }

    fn get_segments_for_output<T, U : FnMut(&[&[Word]]) -> T>(&self, cont : U) -> T {
        self.arena().get_segments_for_output(cont)
    }

    fn get_cap_table<'a>(&'a self) -> &'a [Option<Box<ClientHook+Send>>] {
        self.arena().get_cap_table()
    }
}

pub struct MallocMessageBuilder {
    arena : Box<BuilderArena>,
}

impl Drop for MallocMessageBuilder {
    fn drop(&mut self) { }
}

unsafe impl Send for MallocMessageBuilder {}

impl MallocMessageBuilder {

    pub fn new(options : BuilderOptions) -> MallocMessageBuilder {
        let arena = BuilderArena::new(options.allocation_strategy,
                                      NumWords(options.first_segment_words),
                                      options.fail_fast);

        MallocMessageBuilder { arena : arena }
    }

    pub fn new_default() -> MallocMessageBuilder {
        MallocMessageBuilder::new(BuilderOptions::new())
    }

}

impl MessageBuilder for MallocMessageBuilder {
    fn mut_arena(&mut self) -> &mut BuilderArena {
        &mut *self.arena
    }
    fn arena(&self) -> &BuilderArena {
        & *self.arena
    }
}


pub struct ScratchSpaceMallocMessageBuilder<'a> {
    arena : Box<BuilderArena>,
    scratch_space : &'a mut [Word],
}


// TODO: figure out why rust thinks this is unsafe.
#[unsafe_destructor]
impl <'a> Drop for ScratchSpaceMallocMessageBuilder<'a> {
    fn drop(&mut self) {
        let ptr = self.scratch_space.as_mut_ptr();
        self.get_segments_for_output(|segments| {
                unsafe {
                    std::ptr::zero_memory(ptr, segments[0].len());
                }
            });
    }
}


impl <'a> ScratchSpaceMallocMessageBuilder<'a> {

    pub fn new<'b>(scratch_space : &'b mut [Word], options : BuilderOptions)
               -> ScratchSpaceMallocMessageBuilder<'b> {
        let arena = BuilderArena::new(options.allocation_strategy, ZeroedWords(scratch_space),
                                      options.fail_fast);

        ScratchSpaceMallocMessageBuilder { arena : arena, scratch_space : scratch_space }
    }

    pub fn new_default<'b>(scratch_space : &'b mut [Word]) -> ScratchSpaceMallocMessageBuilder<'b> {
        ScratchSpaceMallocMessageBuilder::new(scratch_space, BuilderOptions::new())
    }

}

impl <'b> MessageBuilder for ScratchSpaceMallocMessageBuilder<'b> {
    fn mut_arena(&mut self) -> &mut BuilderArena {
        &mut *self.arena
    }
    fn arena(&self) -> &BuilderArena {
        & *self.arena
    }
}
