pipeline {
  agent { label 'UbuntuVM' }
  options { skipDefaultCheckout() }
  parameters {
    gitParameter(name: 'GIT_TAG',
                 type: 'PT_BRANCH_TAG',
                 description: 'The Git tag to checkout. If not specified "master" will be checkout.',
                 defaultValue: 'jenkins-tests')
    string(name: 'DOCKER_TAG',
           description: 'An extra Docker tag (e.g. "latest"). By default GIT_TAG will also be used as Docker tag',
           defaultValue: '')
    environment {
        LABEL = get_label()
    }
  }

  stages {
    stage('[MacMini] Checkout Git TAG') {
      agent { label 'MacMini' }
      steps {
        cleanWs()
        checkout([$class: 'GitSCM',
                  branches: [[name: "${params.GIT_TAG}"]],
                  doGenerateSubmoduleConfigurations: false,
                  extensions: [],
                  gitTool: 'Default',
                  submoduleCfg: [],
                  userRemoteConfigs: [[url: 'https://github.com/eclipse-zenoh/zenoh.git']]
                ])
      }
    }
    stage('[MacMini] Update Rust env') {
      agent { label 'MacMini' }
      steps {
        sh '''
        env
        echo "Building eclipse-zenoh-${LABEL}"
        rustup update
        '''
      }
    }
    stage('[MacMini] Build') {
      agent { label 'MacMini' }
      steps {
        sh '''
        cargo build --release --all-targets
        '''
      }
    }
    // stage('[MacMini] Tests') {
    //   agent { label 'MacMini' }
    //   steps {
    //     sh '''
    //     cargo test --verbose
    //     '''
    //   }
    // }
    stage('[MacMini] MacOS Package') {
      agent { label 'MacMini' }
      steps {
        sh '''
        tar -czvf eclipse-zenoh-${LABEL}-macosx-x86-64.tgz --strip-components 2 target/release/zenohd target/release/*.dylib
        tar -czvf eclipse-zenoh-${LABEL}-examples-macosx-x86-64.tgz --exclude 'target/release/examples/*.*' --strip-components 3 target/release/examples/*
        '''
        stash includes: 'eclipse-zenoh-*-macosx-x86-64.tgz', name: 'zenohMacOS'
      }
    }

    stage('[MacMini] Docker build') {
      agent { label 'MacMini' }
      steps {
        sh '''
        RUSTFLAGS='-C target-feature=-crt-static' cargo build --release --target=x86_64-unknown-linux-musl
        if [ -n "${DOCKER_TAG}" ]; then
          export EXTRA_TAG="-t eclipse/zenoh:${DOCKER_TAG}"
        fi
        docker build -t eclipse/zenoh:${LABEL} ${EXTRA_TAG} .
        '''
      }
    }
    // stage('[MacMini] Docker publish') {
    //   agent { label 'MacMini' }
    //   steps {
    //     withCredentials([usernamePassword(credentialsId: 'dockerhub-bot',
    //         passwordVariable: 'DOCKER_HUB_CREDS_PSW', usernameVariable: 'DOCKER_HUB_CREDS_USR')])
    //     {
    //       sh '''
    //       docker login -u ${DOCKER_HUB_CREDS_USR} -p ${DOCKER_HUB_CREDS_PSW}
    //       docker push eclipse/zenoh
    //       docker logout
    //       '''
    //     }
    //   }
    // }

    stage('[MacMini] manylinux2010 x64 build') {
      agent { label 'MacMini' }
      steps {
        sh '''
        docker run --init --rm -v $(pwd):/workdir -w /workdir adlinktech/manylinux2010-x64-rust-nightly \
            /bin/bash -c "\
            cargo build --release --bins --lib --examples --target-dir=target/manylinux2010-x64 && \
            cargo deb -p zenoh-router -o target/manylinux2010-x64 && \
            cargo deb -p zplugin-http -o target/manylinux2010-x64 && \
            cargo deb -p zplugin_storages -o target/manylinux2010-x64 \
            "
        '''
      }
    }
    stage('[MacMini] manylinux2010 x64 Package') {
      agent { label 'MacMini' }
      steps {
        sh '''
        tar -czvf eclipse-zenoh-${LABEL}-manylinux2010-x64.tgz --strip-components 3 target/manylinux2010-x64/release/zenohd target/manylinux2010-x64/release/*.so
        tar -czvf eclipse-zenoh-${LABEL}-examples-manylinux2010-x64.tgz --exclude 'target/manylinux2010-x64/release/examples/*.*' --exclude 'target/manylinux2010-x64/release/examples/*-*' --strip-components 4 target/manylinux2010-x64/release/examples/*
        '''
        stash includes: 'eclipse-zenoh-*-manylinux2010-x64.tgz, target/manylinux2010-x64/*.deb', name: 'zenohManylinux-x64'
      }
    }

    stage('[MacMini] manylinux2010 i686 build') {
      agent { label 'MacMini' }
      steps {
        sh '''
        docker run --init --rm -v $(pwd):/workdir -w /workdir adlinktech/manylinux2010-i686-rust-nightly \
            /bin/bash -c "\
            cargo build --release --bins --lib --examples --target-dir=target/manylinux2010-i686 && \
            cargo deb -p zenoh-router -o target/manylinux2010-i686 && \
            cargo deb -p zplugin-http -o target/manylinux2010-i686 && \
            cargo deb -p zplugin_storages -o target/manylinux2010-i686 \
            "
        '''
      }
    }
    stage('[MacMini] manylinux2010 i686 Package') {
      agent { label 'MacMini' }
      steps {
        sh '''
        tar -czvf eclipse-zenoh-${LABEL}-manylinux2010-i686.tgz --strip-components 3 target/manylinux2010-i686/release/zenohd target/manylinux2010-i686/release/*.so
        tar -czvf eclipse-zenoh-${LABEL}-examples-manylinux2010-i686.tgz --exclude 'target/manylinux2010-i686/release/examples/*.*' --exclude 'target/manylinux2010-i686/release/examples/*-*' --strip-components 4 target/manylinux2010-i686/release/examples/*
        '''
        stash includes: 'eclipse-zenoh-*-manylinux2010-i686.tgz, target/manylinux2010-i686/*.deb', name: 'zenohManylinux-i686'
      }
    }

    stage('Deploy to to download.eclipse.org') {
      steps {
        // Unstash MacOS package to be deployed
        unstash 'zenohMacOS'
        unstash 'zenohManylinux-x64'
        unstash 'zenohManylinux-i686'
        // sshagent ( ['projects-storage.eclipse.org-bot-ssh']) {
        //   sh '''
        //   ssh genie.zenoh@projects-storage.eclipse.org mkdir -p /home/data/httpd/download.eclipse.org/zenoh/zenoh/${LABEL}
        //   ssh genie.zenoh@projects-storage.eclipse.org ls -al /home/data/httpd/download.eclipse.org/zenoh/zenoh/${LABEL}
        //   scp eclipse-zenoh-${LABEL}-*.tgz genie.zenoh@projects-storage.eclipse.org:/home/data/httpd/download.eclipse.org/zenoh/zenoh/${LABEL}/
        //   '''
        // }
      }
    }
  }

  post {
    success {
        archiveArtifacts artifacts: 'eclipse-zenoh-${LABEL}-*.tgz, *.deb', fingerprint: true
    }
  }
}

def get_label() {
    node('master') {
        return env.GIT_TAG.startsWith('origin/') ? env.GIT_TAG.minus('origin/') : env.GIT_TAG
    }
}