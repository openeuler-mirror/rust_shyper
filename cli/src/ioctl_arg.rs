pub const IOCTL_SYS: usize = 0x10;

// ioctl_sys_event
pub const IOCTL_SYS_GET_STATE: usize = 0;
pub const IOCTL_SYS_RECEIVE_MSG: usize = 1;
pub const IOCTL_SYS_INIT_USR_PID: usize = 2;
pub const IOCTL_SYS_GET_SEND_IDX: usize = 3;
pub const IOCTL_SYS_GET_VMID: usize = 4;
pub const IOCTL_SYS_SET_KERNEL_IMG_NAME: usize = 5;
pub const IOCTL_SYS_GET_KERNEL_IMG_NAME: usize = 6;
pub const IOCTL_SYS_APPEND_MED_BLK: usize = 0x10;
