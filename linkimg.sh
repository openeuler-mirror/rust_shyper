#!/bin/bash
set -e

RUST_SYHPER=
MVM_IMAGE=
TOOLCHAIN_LD=
LD_FILE=
TEXT_START=
OUTPUT_FILE="a.out"

function usage {
        cat << EOM
Usage: ./$(basename $BASH_SOURCE) [OPTIONS]
This script is used to link Rust_Shyper and linux image.
It supports following options.
OPTIONS:
        -h | --help                             
            Displays this help
        -i | --image      <Rust_Shyper_Path>   
            Specify the hypervisor path
        -m | --mvm        <MVM_image_path>
            Specify the mvm path
        -t | --toolchain  <toolchain_ld>
            Specify the toolchain of the ld
        -f | --file       <ld_file_path>
            Specify the ld file path
        -s | --text-start <value>

        -o | --output     <outdir>                            
            Creates kernel build output in <outdir>
EOM
}

# parse input parameters
function parse_input_param {
	while [ $# -gt 0 ]; do
		case ${1} in
			-h | --help)
				usage
				exit 0
				;;
            -i | --image)
				RUST_SYHPER="${2}"
				shift 2
				;;
            -m | --mvm)
				MVM_IMAGE="${2}"
				shift 2
				;;
			-t | --toolchain)
				TOOLCHAIN_LD="${2}"
				shift 2
				;;
            -f | --file)
				LD_FILE="${2}"
				shift 2
				;;
            -s | --text-start)
                TEXT_START="${2}"
                shift 2
                ;;   
            -o | --output)
                OUTPUT_FILE="${2}"
                shift 2
                ;; 
			*)
				echo "Error: Invalid option ${1}"
				usage
				exit 1
				;;
			esac
	done
}

function check_params {
    if [ -z "${RUST_SYHPER}" ]; then
        echo "Please enter the hypervisor image by -i"
        exit 0
    fi

    if [ -z "${MVM_IMAGE}" ]; then
        echo "Please enter the mvm image by -m"
        exit 0
    fi

    if [ -z "${TOOLCHAIN_LD}" ]; then
        echo "Please enter the toolchain parameter by -t"
        exit 0
    fi

    if [ -z "${LD_FILE}" ]; then
        echo "Please enter the ld file parameter by -f"
        exit 0
    fi

    if [ -z "${TEXT_START}" ]; then
        echo "Please enter the symbol TEXT_START parameter by -s"
        exit 0
    fi
}

function link_files {
    echo "Linking Rust-shyper and image..."

    cp "${MVM_IMAGE}" vm0img

    "${TOOLCHAIN_LD}" "${RUST_SYHPER}" -T "${LD_FILE}" \
        --defsym TEXT_START="${TEXT_START}" -o "${OUTPUT_FILE}"

    if [ "${MVM_IMAGE}" != "vm0img" ]; then
        rm -f vm0img
    fi

    if [ ! -f "${OUTPUT_FILE}" ]; then
        echo "Error: Missing image ${OUTPUT_FILE}"
        exit 1
    fi

    echo "Link files successfully."
}

parse_input_param "$@"
check_params
link_files