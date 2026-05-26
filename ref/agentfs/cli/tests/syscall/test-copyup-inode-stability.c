#define _GNU_SOURCE
#include "test-common.h"
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>

/**
 * Test for inode stability after copy-up in overlay filesystem.
 *
 * When a file is copied from the base layer to the delta layer (copy-up),
 * the kernel caches the original inode number. If we return a different
 * inode after copy-up, the kernel's cache becomes inconsistent, causing
 * ENOENT errors or other failures.
 *
 * This test verifies that:
 * 1. stat() returns the same inode before and after copy-up
 * 2. Hard links to copied-up files share the same inode
 * 3. lstat() also returns consistent inodes
 * 4. Multiple hard links all report the same inode
 *
 * Related to Linux overlayfs's trusted.overlay.origin mechanism.
 */

int test_copyup_inode_stability(const char *base_path) {
    char orig_path[512], link1_path[512], link2_path[512];
    struct stat st_before, st_after, st_link1, st_link2;
    int result, fd;
    ino_t original_ino;

    snprintf(orig_path, sizeof(orig_path), "%s/copyup_test_file.txt", base_path);
    snprintf(link1_path, sizeof(link1_path), "%s/test_copyup_link1", base_path);
    snprintf(link2_path, sizeof(link2_path), "%s/test_copyup_link2", base_path);

    /* Clean up any previous test files */
    unlink(link1_path);
    unlink(link2_path);
    unlink(orig_path);

    /* Create the test file - this ensures we have a clean file for this test */
    fd = open(orig_path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    TEST_ASSERT_ERRNO(fd >= 0, "create test file should succeed");
    result = write(fd, "copyup test content\n", 20);
    TEST_ASSERT_ERRNO(result == 20, "write test content should succeed");
    close(fd);

    /*
     * Test 1: Get the original inode before any copy-up operation.
     * This file should exist in the base layer.
     */
    result = stat(orig_path, &st_before);
    TEST_ASSERT_ERRNO(result == 0, "stat on original file should succeed");
    original_ino = st_before.st_ino;
    TEST_ASSERT(original_ino > 0, "original inode should be valid");

    /*
     * Test 2: Create a hard link to the file.
     * In an overlay filesystem, this triggers copy-up: the file is copied
     * from base to delta layer. The bug was that after copy-up, stat()
     * would return the new delta inode instead of the original base inode.
     */
    result = link(orig_path, link1_path);
    if (result < 0 && (errno == ENOSYS || errno == EOPNOTSUPP)) {
        printf("  (Skipping copy-up inode stability test - link syscall not supported)\n");
        return 0;
    }
    TEST_ASSERT_ERRNO(result == 0, "link() should succeed");

    /*
     * Test 3: THE CRITICAL TEST - stat() on original file after copy-up.
     * The inode MUST be the same as before. If it changes, the kernel's
     * inode cache becomes inconsistent with reality.
     */
    result = stat(orig_path, &st_after);
    TEST_ASSERT_ERRNO(result == 0, "stat on original file after link should succeed");
    if (st_after.st_ino != original_ino) {
        fprintf(stderr, "  inode changed: was %lu, now %lu\n",
                (unsigned long)original_ino, (unsigned long)st_after.st_ino);
    }
    TEST_ASSERT(st_after.st_ino == original_ino,
        "inode must remain stable after copy-up");

    /*
     * Test 4: stat() on the hard link should return the same inode.
     * Hard links by definition share the same inode.
     */
    result = stat(link1_path, &st_link1);
    TEST_ASSERT_ERRNO(result == 0, "stat on hard link should succeed");
    if (st_link1.st_ino != original_ino) {
        fprintf(stderr, "  hard link inode mismatch: expected %lu, got %lu\n",
                (unsigned long)original_ino, (unsigned long)st_link1.st_ino);
    }
    TEST_ASSERT(st_link1.st_ino == original_ino,
        "hard link must have same inode as original");

    /*
     * Test 5: lstat() should also return consistent inodes.
     * Even though these aren't symlinks, lstat() is often used and must
     * also return the correct (original) inode.
     */
    result = lstat(orig_path, &st_after);
    TEST_ASSERT_ERRNO(result == 0, "lstat on original file should succeed");
    TEST_ASSERT(st_after.st_ino == original_ino,
        "lstat inode must match original after copy-up");

    result = lstat(link1_path, &st_link1);
    TEST_ASSERT_ERRNO(result == 0, "lstat on hard link should succeed");
    TEST_ASSERT(st_link1.st_ino == original_ino,
        "lstat on hard link must return same inode as original");

    /*
     * Test 6: Create a second hard link and verify all three paths
     * report the same inode.
     */
    result = link(orig_path, link2_path);
    TEST_ASSERT_ERRNO(result == 0, "creating second hard link should succeed");

    result = stat(link2_path, &st_link2);
    TEST_ASSERT_ERRNO(result == 0, "stat on second hard link should succeed");
    if (st_link2.st_ino != original_ino) {
        fprintf(stderr, "  second hard link inode mismatch: expected %lu, got %lu\n",
                (unsigned long)original_ino, (unsigned long)st_link2.st_ino);
    }
    TEST_ASSERT(st_link2.st_ino == original_ino,
        "second hard link must have same inode");

    /* Re-check original and first link still have correct inode */
    result = stat(orig_path, &st_after);
    TEST_ASSERT_ERRNO(result == 0, "stat on original after second link should succeed");
    TEST_ASSERT(st_after.st_ino == original_ino,
        "original inode must still be stable after multiple links");

    result = stat(link1_path, &st_link1);
    TEST_ASSERT_ERRNO(result == 0, "stat on first link after second link should succeed");
    TEST_ASSERT(st_link1.st_ino == original_ino,
        "first link inode must still match original");

    /*
     * Test 7: Verify link count is consistent.
     * After creating two hard links, nlink should be at least 3.
     */
    if (st_after.st_nlink < 3) {
        fprintf(stderr, "  nlink too low: expected >= 3, got %lu\n",
                (unsigned long)st_after.st_nlink);
    }
    TEST_ASSERT(st_after.st_nlink >= 3,
        "nlink should be at least 3 after creating two hard links");

    /*
     * Test 8: Delete one link and verify inodes remain stable.
     */
    result = unlink(link1_path);
    TEST_ASSERT_ERRNO(result == 0, "unlink first hard link should succeed");

    result = stat(orig_path, &st_after);
    TEST_ASSERT_ERRNO(result == 0, "stat on original after unlink should succeed");
    TEST_ASSERT(st_after.st_ino == original_ino,
        "original inode must remain stable after unlinking a hard link");

    result = stat(link2_path, &st_link2);
    TEST_ASSERT_ERRNO(result == 0, "stat on remaining link should succeed");
    TEST_ASSERT(st_link2.st_ino == original_ino,
        "remaining link must still have original inode");

    /*
     * Test 9: fstat() on open file descriptor should also return stable inode.
     */
    fd = open(orig_path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "open original file should succeed");

    result = fstat(fd, &st_after);
    TEST_ASSERT_ERRNO(result == 0, "fstat on open fd should succeed");
    if (st_after.st_ino != original_ino) {
        fprintf(stderr, "  fstat inode mismatch: expected %lu, got %lu\n",
                (unsigned long)original_ino, (unsigned long)st_after.st_ino);
    }
    TEST_ASSERT(st_after.st_ino == original_ino,
        "fstat must return stable inode");
    close(fd);

    /* Also check fstat on the link */
    fd = open(link2_path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "open hard link should succeed");

    result = fstat(fd, &st_link2);
    TEST_ASSERT_ERRNO(result == 0, "fstat on hard link fd should succeed");
    TEST_ASSERT(st_link2.st_ino == original_ino,
        "fstat on hard link must return same inode as original");
    close(fd);

    /* Clean up */
    unlink(link2_path);
    unlink(orig_path);

    return 0;
}
