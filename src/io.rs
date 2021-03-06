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

//! Input / output.

use std::old_io::{Reader, Writer, IoResult};

pub fn read_at_least<R : Reader>(reader : &mut R,
                                 buf: &mut [u8],
                                 min_bytes : usize) -> IoResult<usize> {
    let mut pos = 0;
    let buf_len = buf.len();
    while pos < min_bytes {
        let buf1 = &mut buf[pos .. buf_len];
        let n = try!(reader.read(buf1));
        pos += n;
    }
    return Ok(pos);
}

pub trait BufferedInputStream : Reader {
    fn skip(&mut self, bytes : usize) -> IoResult<()>;
    unsafe fn get_read_buffer(&mut self) -> IoResult<(*const u8, *const u8)>;
}

pub struct BufferedInputStreamWrapper<'a, R: 'a> {
    inner : &'a mut R,
    buf : Vec<u8>,
    pos : usize,
    cap : usize
}

impl <'a, R> BufferedInputStreamWrapper<'a, R> {
    pub fn new<'b> (r : &'b mut R) -> BufferedInputStreamWrapper<'b, R> {
        let mut result = BufferedInputStreamWrapper {
            inner : r,
            buf : Vec::with_capacity(8192),
            pos : 0,
            cap : 0
        };
        unsafe {
            result.buf.set_len(8192)
        }
        return result;
    }
}

impl<'a, R: Reader> BufferedInputStream for BufferedInputStreamWrapper<'a, R> {

   fn skip(&mut self, mut bytes : usize) -> IoResult<()> {
        let available = self.cap - self.pos;
        if bytes <= available {
            self.pos += bytes;
        } else {
            bytes -= available;
            if bytes <= self.buf.len() {
                //# Read the next buffer-full.
                let n = try!(read_at_least(self.inner, self.buf.as_mut_slice(), bytes));
                self.pos = bytes;
                self.cap = n;
            } else {
                //# Forward large skip to the underlying stream.
                panic!("TODO")
            }
        }
        Ok(())
    }

    unsafe fn get_read_buffer(&mut self) -> IoResult<(*const u8, *const u8)> {
        if self.cap - self.pos == 0 {
            let n = try!(read_at_least(self.inner, self.buf.as_mut_slice(), 1));
            self.cap = n;
            self.pos = 0;
        }
        Ok((self.buf.get_unchecked(self.pos) as *const u8,
            self.buf.get_unchecked(self.cap) as *const u8))
    }
}

impl<'a, R: Reader> Reader for BufferedInputStreamWrapper<'a, R> {
    fn read(&mut self, dst: &mut [u8]) -> IoResult<usize> {
        let mut num_bytes = dst.len();
        if num_bytes <= self.cap - self.pos {
            //# Serve from the current buffer.
            ::std::slice::bytes::copy_memory(dst,
                                           &self.buf[self.pos .. self.pos + num_bytes]);
            self.pos += num_bytes;
            return Ok(num_bytes);
        } else {
            //# Copy current available into destination.

            ::std::slice::bytes::copy_memory(dst,
                                             &self.buf[self.pos .. self.cap]);
            let from_first_buffer = self.cap - self.pos;

            let dst1 = &mut dst[from_first_buffer .. num_bytes];
            num_bytes -= from_first_buffer;
            if num_bytes <= self.buf.len() {
                //# Read the next buffer-full.
                let n = try!(read_at_least(self.inner, self.buf.as_mut_slice(), num_bytes));
                ::std::slice::bytes::copy_memory(dst1,
                                                 &self.buf[0 .. num_bytes]);
                self.cap = n;
                self.pos = num_bytes;
                return Ok(from_first_buffer + num_bytes);
            } else {
                //# Forward large read to the underlying stream.
                self.pos = 0;
                self.cap = 0;
                return Ok(from_first_buffer + try!(read_at_least(self.inner, dst1, num_bytes)));
            }
        }
    }
}

pub struct ArrayInputStream<'a> {
    array : &'a [u8]
}

impl <'a> ArrayInputStream<'a> {
    pub fn new<'b>(array : &'b [u8]) -> ArrayInputStream<'b> {
        ArrayInputStream { array : array }
    }
}

impl <'a> Reader for ArrayInputStream<'a> {
    fn read(&mut self, dst: &mut [u8]) -> Result<usize, ::std::old_io::IoError> {
        let n = ::std::cmp::min(dst.len(), self.array.len());
        unsafe { ::std::ptr::copy_nonoverlapping_memory(dst.as_mut_ptr(), self.array.as_ptr(), n) }
        self.array = &self.array[n ..];
        Ok(n)
    }
}

impl <'a> BufferedInputStream for ArrayInputStream<'a> {
    fn skip(&mut self, bytes : usize) -> IoResult<()> {
        assert!(self.array.len() >= bytes,
                "ArrayInputStream ended prematurely.");
        self.array = &self.array[bytes ..];
        Ok(())
    }
    unsafe fn get_read_buffer(&mut self) -> IoResult<(*const u8, *const u8)> {
        let len = self.array.len();
        Ok((self.array.as_ptr() as *const u8,
           self.array.get_unchecked(len) as *const u8))
    }
}

pub trait BufferedOutputStream : Writer {
    unsafe fn get_write_buffer(&mut self) -> (*mut u8, *mut u8);
    unsafe fn write_ptr(&mut self, ptr: *mut u8, size: usize) -> IoResult<()>;
}

pub struct BufferedOutputStreamWrapper<'a, W:'a> {
    inner: &'a mut W,
    buf: Vec<u8>,
    pos: usize
}

impl <'a, W> BufferedOutputStreamWrapper<'a, W> {
    pub fn new<'b> (w : &'b mut W) -> BufferedOutputStreamWrapper<'b, W> {
        let mut result = BufferedOutputStreamWrapper {
            inner: w,
            buf : Vec::with_capacity(8192),
            pos : 0
        };
        unsafe {
            result.buf.set_len(8192);
        }
        return result;
    }
}

impl<'a, W: Writer> BufferedOutputStream for BufferedOutputStreamWrapper<'a, W> {
    #[inline]
    unsafe fn get_write_buffer(&mut self) -> (*mut u8, *mut u8) {
        let len = self.buf.len();
        (self.buf.get_unchecked_mut(self.pos) as *mut u8,
         self.buf.get_unchecked_mut(len) as *mut u8)
    }

    #[inline]
    unsafe fn write_ptr(&mut self, ptr: *mut u8, size: usize) -> IoResult<()> {
        let easy_case = ptr == self.buf.get_unchecked_mut(self.pos) as *mut u8;
        if easy_case {
            self.pos += size;
            Ok(())
        } else {
            let buf = ::std::slice::from_raw_parts_mut::<u8>(ptr, size);
            self.write_all(buf)
        }
    }

}


impl<'a, W: Writer> Writer for BufferedOutputStreamWrapper<'a, W> {
    fn write_all(&mut self, buf: &[u8]) -> IoResult<()> {
        let available = self.buf.len() - self.pos;
        let mut size = buf.len();
        if size <= available {
            let dst = &mut self.buf.as_mut_slice()[self.pos ..];
            ::std::slice::bytes::copy_memory(dst, buf);
            self.pos += size;
        } else if size <= self.buf.len() {
            //# Too much for this buffer, but not a full buffer's
            //# worth, so we'll go ahead and copy.
            {
                let dst = &mut self.buf.as_mut_slice()[self.pos ..];
                ::std::slice::bytes::copy_memory(dst, &buf[0 .. available]);
            }
            try!(self.inner.write_all(self.buf.as_mut_slice()));

            size -= available;
            let src = &buf[available ..];
            let dst = &mut self.buf.as_mut_slice()[0 ..];
            ::std::slice::bytes::copy_memory(dst, src);
            self.pos = size;
        } else {
            //# Writing so much data that we might as well write
            //# directly to avoid a copy.
            try!(self.inner.write_all(&self.buf[0 .. self.pos]));
            self.pos = 0;
            try!(self.inner.write_all(buf));
        }
        return Ok(());
    }

    fn flush(&mut self) -> IoResult<()> {
        if self.pos > 0 {
            try!(self.inner.write_all(&self.buf[0 .. self.pos]));
            self.pos = 0;
        }
        self.inner.flush()
    }
}

pub struct ArrayOutputStream<'a> {
    array : &'a mut [u8],
    fill_pos : usize,
}

impl <'a> ArrayOutputStream<'a> {
    pub fn new<'b>(array : &'b mut [u8]) -> ArrayOutputStream<'b> {
        ArrayOutputStream {
            array : array,
            fill_pos : 0
        }
    }
}

impl <'a> Writer for ArrayOutputStream<'a> {
    fn write_all(&mut self, buf: &[u8]) -> IoResult<()> {
        assert!(buf.len() <= self.array.len() - self.fill_pos,
                "ArrayOutputStream's backing array was not large enough for the data written.");
        unsafe { ::std::ptr::copy_nonoverlapping_memory(
            self.array.get_unchecked_mut(self.fill_pos),
            buf.as_ptr(),
            buf.len());  }
        self.fill_pos += buf.len();
        Ok(())
    }
}

impl <'a> BufferedOutputStream for ArrayOutputStream<'a> {
    unsafe fn get_write_buffer(&mut self) -> (*mut u8, *mut u8) {
        let len = self.array.len();
        (self.array.get_unchecked_mut(self.fill_pos) as *mut u8,
         self.array.get_unchecked_mut(len) as *mut u8)
    }
    unsafe fn write_ptr(&mut self, ptr: *mut u8, size: usize) -> IoResult<()> {
        let easy_case = ptr == self.array.get_unchecked_mut(self.fill_pos) as *mut u8;
        if easy_case {
            self.fill_pos += size;
            Ok(())
        } else {
            let buf = ::std::slice::from_raw_parts_mut::<u8>(ptr, size);
            self.write_all(buf)
        }
    }
}
